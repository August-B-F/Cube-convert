use std::fs;
use std::path::Path;

use super::{shared, CancelFlag, ProgressTx};

pub fn convert_bpm(
    file_path: &Path,
    is_folder: bool,
    tx: ProgressTx,
    cancel: CancelFlag,
) -> Result<(), String> {
    shared::process_files(file_path, is_folder, tx, cancel, |pdf, name| {
        let out = pdf.with_file_name(format!("{name}.mp3"));
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

        let framerate = 3000u32;
        let amplitude = 32000i16;
        let total_duration_secs = 12.0 * 60.0f64;
        let pulse_duration = (total_duration_secs / bpm_list.len() as f64) * 1.0255;
        let twelve_min_samples = (total_duration_secs * framerate as f64) as usize;

        let tmp_dir = shared::make_temp_dir("bpm")?;
        let tmp = tmp_dir.join(format!("{name}_tmp.wav"));
        {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: framerate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut w = hound::WavWriter::create(&tmp, spec)
                .map_err(|e| format!("create {}: {e}", tmp.display()))?;
            let mut written = 0usize;
            let mut write_n = |w: &mut hound::WavWriter<_>, sample: i16, n: usize, written: &mut usize| -> Result<(), String> {
                for _ in 0..n {
                    if *written >= twelve_min_samples { break; }
                    w.write_sample(sample).map_err(|e| e.to_string())?;
                    w.write_sample(sample).map_err(|e| e.to_string())?;
                    *written += 1;
                }
                Ok(())
            };

            for &bpm in &bpm_list {
                if bpm == 0 || written >= twelve_min_samples { break; }
                let pulses = ((bpm as f64 / 60.0) * pulse_duration) as usize;
                let pulse_len = ((60.0 / bpm as f64 * 2.0) * framerate as f64) as usize;
                let boosted = (amplitude as f64 * 3.162).clamp(i16::MIN as f64, i16::MAX as f64) as i16;
                for _ in 0..pulses {
                    write_n(&mut w, 0, 1, &mut written)?;
                    write_n(&mut w, boosted, pulse_len, &mut written)?;
                }
            }
            let last_bpm = *bpm_list.last().unwrap();
            let pulse_len = ((60.0 / last_bpm as f64 * 2.0) * framerate as f64) as usize;
            let boosted = (amplitude as f64 * 3.162).clamp(i16::MIN as f64, i16::MAX as f64) as i16;
            while written < twelve_min_samples {
                write_n(&mut w, boosted, pulse_len, &mut written)?;
            }
            w.finalize().map_err(|e| e.to_string())?;
        }

        let ffmpeg = shared::ffmpeg_bin();
        let preset = shared::ffmpeg_preset();
        let args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-i".into(), tmp.to_string_lossy().to_string(),
            "-vn".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
            "-b:a".into(), "320k".into(), "-codec:a".into(), "libmp3lame".into(),
            "-preset".into(), preset,
            out.to_string_lossy().to_string(),
        ];
        shared::run_cmd(&ffmpeg, &args)?;

        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&tmp_dir);
        Ok(())
    })
}
