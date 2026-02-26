use std::fs;
use std::path::Path;

use super::{shared, CancelFlag, ProgressTx};

/// Synthesizes a clean, realistic double-thump heartbeat.
/// Returns a single beat cycle adjusted for the instantaneous BPM.
fn generate_single_beat(current_bpm: f32, sample_rate: u32) -> Vec<f32> {
    let beat_len_sec = 60.0 / current_bpm;
    let num_samples = (beat_len_sec * sample_rate as f32) as usize;
    let mut beat = vec![0.0; num_samples];

    let lub_len_sec = 0.1_f32.min(beat_len_sec * 0.3);
    let gap_len_sec = 0.05_f32.min(beat_len_sec * 0.1);
    let dub_len_sec = 0.08_f32.min(beat_len_sec * 0.25);

    let lub_samples = (lub_len_sec * sample_rate as f32) as usize;
    let gap_samples = (gap_len_sec * sample_rate as f32) as usize;
    let dub_samples = (dub_len_sec * sample_rate as f32) as usize;

    let lub_freq = 45.0; 
    let dub_freq = 55.0; 

    // Louder amplitude multiplier
    let amp_mult = 1.8;

    // 1. Generate S1 (Lub)
    for i in 0..lub_samples {
        if i >= num_samples { break; }
        let t = i as f32 / sample_rate as f32;
        let env = (std::f32::consts::PI * i as f32 / lub_samples as f32).sin(); 
        let wave = (2.0 * std::f32::consts::PI * lub_freq * t).sin();
        beat[i] = (wave * env * 1.0 * amp_mult).clamp(-1.0, 1.0);
    }

    // 2. Generate S2 (Dub)
    let dub_start = lub_samples + gap_samples;
    for i in 0..dub_samples {
        let idx = dub_start + i;
        if idx >= num_samples { break; }
        let t = i as f32 / sample_rate as f32;
        let env = (std::f32::consts::PI * i as f32 / dub_samples as f32).sin();
        let wave = (2.0 * std::f32::consts::PI * dub_freq * t).sin();
        beat[idx] = (wave * env * 0.75 * amp_mult).clamp(-1.0, 1.0);
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
        
        // Better Parsing: Split by whitespace to separate minute markers (01, 02) from real BPMs
        let mut bpm_list: Vec<f32> = Vec::new();
        for token in text.split_whitespace() {
            // Remove any non-digit characters just in case
            let clean_token: String = token.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(val) = clean_token.parse::<f32>() {
                // Filter out minute markers and unreasonably low BPMs
                if val >= 60.0 {
                    bpm_list.push(val);
                }
            }
        }

        if bpm_list.is_empty() {
            return Err("No valid BPM data found (>60 BPM)".into());
        }

        let sample_rate = 44100u32;
        let total_duration_secs = 12.0 * 60.0;
        let target_samples = (total_duration_secs * sample_rate as f64) as usize;
        
        let tmp_dir = shared::make_temp_dir("bpm")?;
        let tmp = tmp_dir.join(format!("{name}_tmp.wav"));
        {
            let spec = hound::WavSpec {
                channels: 2, 
                sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&tmp, spec)
                .map_err(|e| format!("create {}: {e}", tmp.display()))?;

            let mut total_written = 0usize;
            let n_bpms = bpm_list.len();
            
            // Continuous generation loop
            while total_written < target_samples {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("Cancelled.".into());
                }

                // Determine exact time in the timeline
                let current_time_sec = total_written as f64 / sample_rate as f64;
                
                // Map current time to the fractional index of our BPM array
                let progress = current_time_sec / total_duration_secs;
                let exact_index = progress * (n_bpms.saturating_sub(1) as f64);
                
                let idx1 = exact_index.floor() as usize;
                let idx2 = (idx1 + 1).min(n_bpms - 1);
                let frac = exact_index - idx1 as f64;

                // Smooth linear interpolation between the two BPM points
                let current_bpm = bpm_list[idx1] + (bpm_list[idx2] - bpm_list[idx1]) * frac as f32;

                // Generate exactly one beat cycle based on the *current* smooth BPM
                let beat_data = generate_single_beat(current_bpm, sample_rate);

                for &sample in &beat_data {
                    if total_written >= target_samples { break; }
                    
                    w.write_sample(sample).map_err(|e| e.to_string())?; // Left
                    w.write_sample(sample).map_err(|e| e.to_string())?; // Right
                    
                    total_written += 1;
                }
            }

            w.finalize().map_err(|e| e.to_string())?;
        }

        let _ = prog_tx.send(super::Progress::Update { name: name.to_string(), fraction: 0.5 });

        let preset = shared::ffmpeg_preset();
        let args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
            "-i".into(), tmp.to_string_lossy().to_string(),
            "-vn".into(), "-ar".into(), "44100".into(), "-ac".into(), "2".into(),
            "-b:a".into(), "320k".into(), "-codec:a".into(), "libmp3lame".into(),
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
