use std::fs;
use std::path::Path;

use super::{shared, CancelFlag, ProgressTx};

pub fn convert_bpm(
    file_path: &Path,
    is_folder: bool,
    tx: ProgressTx,
    cancel: CancelFlag,
) -> Result<(), String> {
    shared::process_files(file_path, is_folder, tx, cancel.clone(), |pdf, out_dir, name, prog_tx| {
        let out = out_dir.join(format!("{name}.mp3"));
        let partial_out = out.with_extension("tmp.mp3");
        if out.exists() {
            return Ok(());
        }

        let text = shared::extract_text(pdf)?;
        let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();

        let mut bpm_list: Vec<u32> = Vec::new();
        for chunk in digits.as_bytes().chunks(3) {
            if let Ok(n) = std::str::from_utf8(chunk).unwrap_or("").parse::<u32>() {
                bpm_list.push(n);
            }
        }
        if bpm_list.is_empty() {
            return Err("No BPM data found".into());
        }

        // Must use proper audio sample rate - 3000 caused aliasing/ringing when resampled
        let sample_rate = 44100u32;
        let total_duration_secs = 12.0 * 60.0f64;
        let pulse_duration = (total_duration_secs / bpm_list.len() as f64) * 1.0255;
        let twelve_min_samples = (total_duration_secs * sample_rate as f64) as usize;

        let tmp_dir = shared::make_temp_dir("bpm")?;
        let tmp = tmp_dir.join(format!("{name}_tmp.wav"));
        {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut w = hound::WavWriter::create(&tmp, spec)
                .map_err(|e| format!("create {}: {e}", tmp.display()))?;

            let mut written = 0usize;

            // Heartbeat envelope shape:
            // Two quick bumps (lub-dub) then silence for the rest of the beat period.
            // Each bump is a short sine arch - gives organic thump vs square wave.
            let write_heartbeat = |w: &mut hound::WavWriter<_>, bpm: u32, written: &mut usize| -> Result<(), String> {
                if bpm == 0 { return Ok(()); }
                let beat_samples = (sample_rate as f64 * 60.0 / bpm as f64) as usize;

                // Lub: 8% of beat period
                let lub_len = (beat_samples as f32 * 0.08) as usize;
                // Short gap between lub and dub: 5%
                let gap_len = (beat_samples as f32 * 0.05) as usize;
                // Dub: 6% of beat period (slightly softer)
                let dub_len = (beat_samples as f32 * 0.06) as usize;
                // Rest: remainder of beat in silence
                let rest_len = beat_samples.saturating_sub(lub_len + gap_len + dub_len);

                let amplitude = i16::MAX as f32 * 0.95;

                // Lub - full amplitude sine arch
                for i in 0..lub_len {
                    if *written >= twelve_min_samples { return Ok(()); }
                    let t = i as f32 / lub_len as f32;
                    let env = (t * std::f32::consts::PI).sin(); // 0 -> 1 -> 0
                    let sample = (env * amplitude) as i16;
                    w.write_sample(sample).map_err(|e| e.to_string())?;
                    w.write_sample(sample).map_err(|e| e.to_string())?;
                    *written += 1;
                }

                // Gap
                for _ in 0..gap_len {
                    if *written >= twelve_min_samples { return Ok(()); }
                    w.write_sample(0i16).map_err(|e| e.to_string())?;
                    w.write_sample(0i16).map_err(|e| e.to_string())?;
                    *written += 1;
                }

                // Dub - 70% amplitude
                for i in 0..dub_len {
                    if *written >= twelve_min_samples { return Ok(()); }
                    let t = i as f32 / dub_len as f32;
                    let env = (t * std::f32::consts::PI).sin();
                    let sample = (env * amplitude * 0.7) as i16;
                    w.write_sample(sample).map_err(|e| e.to_string())?;
                    w.write_sample(sample).map_err(|e| e.to_string())?;
                    *written += 1;
                }

                // Silence for rest of beat
                for _ in 0..rest_len {
                    if *written >= twelve_min_samples { return Ok(()); }
                    w.write_sample(0i16).map_err(|e| e.to_string())?;
                    w.write_sample(0i16).map_err(|e| e.to_string())?;
                    *written += 1;
                }

                Ok(())
            };

            for &bpm in &bpm_list {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("Cancelled.".into());
                }
                if written >= twelve_min_samples { break; }

                let pulses = ((bpm as f64 / 60.0) * pulse_duration).round() as usize;
                for _ in 0..pulses {
                    if written >= twelve_min_samples { break; }
                    write_heartbeat(&mut w, bpm, &mut written)?;
                }
            }

            // Fill remaining with last bpm's heartbeat
            let last_bpm = *bpm_list.last().unwrap();
            while written < twelve_min_samples {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("Cancelled.".into());
                }
                write_heartbeat(&mut w, last_bpm, &mut written)?;
            }

            w.finalize().map_err(|e| e.to_string())?;
        }

        let _ = prog_tx.send(super::Progress::Update { name: name.to_string(), fraction: 0.5 });

        let args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
            "-i".into(), tmp.to_string_lossy().to_string(),
            "-vn".into(), "-ar".into(), "44100".into(), "-ac".into(), "2".into(),
            "-b:a".into(), "320k".into(), "-codec:a".into(), "libmp3lame".into(),
            partial_out.to_string_lossy().to_string(),
        ];
        
        let result = shared::run_ffmpeg(&args, None, prog_tx, name, cancel.clone());

        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&tmp_dir);
        
        if result.is_ok() && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = fs::rename(&partial_out, &out);
        } else {
            let _ = fs::remove_file(&partial_out);
        }

        result
    })
}
