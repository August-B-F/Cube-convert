use std::fs;
use std::path::Path;

use super::{shared, CancelFlag, ProgressTx};

pub fn convert_wind(
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
        let mut wind_intensities: Vec<Vec<f32>> = Vec::new();

        for line in text.split('\n') {
            let stripped = if line.len() > 2 { &line[2..] } else { line };
            let day: Vec<f32> = stripped
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect();
            if !day.is_empty() {
                wind_intensities.push(day);
            }
        }

        if wind_intensities.is_empty() {
            return Err("No wind intensity data found".into());
        }
        if wind_intensities.len() >= 25 {
            wind_intensities.truncate(24);
        }

        let wind_path = Path::new("assets/Wind_Loop.wav");
        if !wind_path.exists() {
            return Err("assets/Wind_Loop.wav not found".into());
        }

        let mut reader = hound::WavReader::open(wind_path)
            .map_err(|e| format!("open {}: {e}", wind_path.display()))?;
        let wind_spec = reader.spec();
        let wind_sample_rate = wind_spec.sample_rate as f32;
        let wind_data: Vec<f32> = match wind_spec.sample_format {
            hound::SampleFormat::Float => {
                reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect()
            }
            hound::SampleFormat::Int => reader
                .samples::<i32>()
                .map(|s| s.unwrap_or(0) as f32 / i32::MAX as f32)
                .collect(),
        };
        let n_wind = wind_data.len();
        let wind_duration = n_wind as f32 / wind_sample_rate;

        let sample_rate = 44100u32;
        let duration_per_day = 30.0f32;
        let mut output: Vec<f32> = Vec::new();

        for day in &wind_intensities {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("Cancelled.".into());
            }

            if day.is_empty() { continue; }

            let dur_per_int = duration_per_day / day.len() as f32;
            let samples = (sample_rate as f32 * duration_per_day) as usize;

            // Match Python: intensity_start carries over between samples
            let mut intensity_start = day[0];

            for i in 0..samples {
                let elapsed = i as f32 / sample_rate as f32;
                let intensity_index = (elapsed / dur_per_int) as usize;
                let intensity_index = intensity_index.min(day.len() - 1);
                let intensity_end = day[intensity_index];

                let intensity = if intensity_index == day.len() - 1 {
                    intensity_end
                } else {
                    // transition_fraction = 1.0, so transition_duration == dur_per_int
                    let t = (elapsed - intensity_index as f32 * dur_per_int) / dur_per_int;
                    intensity_start + (intensity_end - intensity_start) * t
                };

                intensity_start = intensity_end;

                // Use wind sample rate to index into wind_data, matching Python's wind_index logic
                let wind_index = ((i as f32 / sample_rate as f32 * wind_sample_rate) as usize) % n_wind;
                let wind_sample = wind_data[wind_index];

                let value = if intensity <= 1.0 {
                    0.0
                } else {
                    wind_sample * intensity / 15.0
                };

                // Python applies +3dB via pydub (factor ~1.412)
                output.push(value * 1.412);
            }
        }

        let tmp_dir = shared::make_temp_dir("wind")?;
        let tmp = tmp_dir.join(format!("{name}_tmp.wav"));
        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&tmp, spec)
                .map_err(|e| format!("create {}: {e}", tmp.display()))?;
            for s in &output {
                w.write_sample(*s).map_err(|e| e.to_string())?;
            }
            w.finalize().map_err(|e| e.to_string())?;
        }

        let _ = prog_tx.send(super::Progress::Update { name: name.to_string(), fraction: 0.5 });

        let args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
            "-i".into(), tmp.to_string_lossy().to_string(),
            "-vn".into(), "-ar".into(), "44100".into(), "-ac".into(), "2".into(),
            "-b:a".into(), "192k".into(), "-codec:a".into(), "libmp3lame".into(),
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
