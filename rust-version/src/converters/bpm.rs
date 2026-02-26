use std::fs;
use std::path::Path;

use super::{shared, CancelFlag, ProgressTx};

/// Synthesizes a clean, realistic double-thump heartbeat for a given BPM.
/// Uses sine envelopes to create a smooth kick-drum style pulse with NO ringing.
fn generate_beat(bpm: f32, sample_rate: u32) -> Vec<f32> {
    let beat_len_sec = 60.0 / bpm;
    let num_samples = (beat_len_sec * sample_rate as f32) as usize;
    let mut beat = vec![0.0; num_samples];

    // Scale durations down if BPM is very high to prevent overlapping
    let lub_len_sec = 0.1_f32.min(beat_len_sec * 0.3);
    let gap_len_sec = 0.05_f32.min(beat_len_sec * 0.1);
    let dub_len_sec = 0.08_f32.min(beat_len_sec * 0.25);

    let lub_samples = (lub_len_sec * sample_rate as f32) as usize;
    let gap_samples = (gap_len_sec * sample_rate as f32) as usize;
    let dub_samples = (dub_len_sec * sample_rate as f32) as usize;

    // Deep, thumping low frequencies
    let lub_freq = 45.0; // The deep 'lub'
    let dub_freq = 55.0; // The slightly higher 'dub'

    // 1. Generate S1 (Lub)
    for i in 0..lub_samples {
        if i >= num_samples { break; }
        let t = i as f32 / sample_rate as f32;
        // Sine window for smooth attack and decay (no clicking)
        let env = (std::f32::consts::PI * i as f32 / lub_samples as f32).sin(); 
        let wave = (2.0 * std::f32::consts::PI * lub_freq * t).sin();
        beat[i] = wave * env * 0.95; // 95% volume
    }

    // 2. Generate S2 (Dub)
    let dub_start = lub_samples + gap_samples;
    for i in 0..dub_samples {
        let idx = dub_start + i;
        if idx >= num_samples { break; }
        let t = i as f32 / sample_rate as f32;
        let env = (std::f32::consts::PI * i as f32 / dub_samples as f32).sin();
        let wave = (2.0 * std::f32::consts::PI * dub_freq * t).sin();
        beat[idx] = wave * env * 0.70; // Dub is slightly quieter
    }

    beat
}

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
                if n > 0 { // Ignore 0 BPM to prevent divide-by-zero
                    bpm_list.push(n);
                }
            }
        }
        if bpm_list.is_empty() {
            return Err("No valid BPM data found".into());
        }

        let sample_rate = 44100u32;
        let total_duration_secs = 12.0 * 60.0;
        let target_samples = (total_duration_secs * sample_rate as f64) as usize;
        
        let time_per_bpm = total_duration_secs / bpm_list.len() as f64;
        let samples_per_bpm_block = (time_per_bpm * sample_rate as f64) as usize;

        let tmp_dir = shared::make_temp_dir("bpm")?;
        let tmp = tmp_dir.join(format!("{name}_tmp.wav"));
        {
            let spec = hound::WavSpec {
                channels: 2, // Stereo output
                sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&tmp, spec)
                .map_err(|e| format!("create {}: {e}", tmp.display()))?;

            let mut total_written = 0usize;

            for &bpm in &bpm_list {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("Cancelled.".into());
                }

                let beat_data = generate_beat(bpm as f32, sample_rate);
                let mut block_written = 0usize;

                // Loop the generated heartbeat until this BPM's time block is full
                while block_written < samples_per_bpm_block && total_written < target_samples {
                    for &sample in &beat_data {
                        if block_written >= samples_per_bpm_block || total_written >= target_samples {
                            break;
                        }
                        
                        w.write_sample(sample).map_err(|e| e.to_string())?; // Left channel
                        w.write_sample(sample).map_err(|e| e.to_string())?; // Right channel
                        
                        block_written += 1;
                        total_written += 1;
                    }
                }
            }

            // Pad with silence if we somehow fell slightly short of exactly 12 minutes
            while total_written < target_samples {
                w.write_sample(0.0f32).map_err(|e| e.to_string())?;
                w.write_sample(0.0f32).map_err(|e| e.to_string())?;
                total_written += 1;
            }

            w.finalize().map_err(|e| e.to_string())?;
        }

        let _ = prog_tx.send(super::Progress::Update { name: name.to_string(), fraction: 0.5 });

        let preset = shared::ffmpeg_preset();
        let args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
            "-i".into(), tmp.to_string_lossy().to_string(),
            "-vn".into(), "-ar".into(), "44100".into(), "-ac".into(), "2".into(),
            "-b:a".into(), "192k".into(), "-codec:a".into(), "libmp3lame".into(),
            "-preset".into(), preset,
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
