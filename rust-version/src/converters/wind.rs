use std::fs;
use std::path::Path;

use super::{shared, CancelFlag, ProgressTx};

pub fn convert_wind(
    file_path: &Path,
    is_folder: bool,
    tx: ProgressTx,
    cancel: CancelFlag,
) -> Result<(), String> {
    shared::process_files(file_path, is_folder, tx, cancel, |pdf, name, prog_tx| {
        let out = pdf.with_file_name(format!("{name}.mp3"));
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
        let wind_data: Vec<f32> = match reader.spec().sample_format {
            hound::SampleFormat::Float => {
                reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect()
            }
            hound::SampleFormat::Int => reader
                .samples::<i32>()
                .map(|s| s.unwrap_or(0) as f32 / i32::MAX as f32)
                .collect(),
        };
        let n_wind = wind_data.len();
        let sample_rate = 44100u32;
        let duration_per_day = 30.0f32;
        let mut output: Vec<f32> = Vec::new();

        for day in &wind_intensities {
            if day.is_empty() { continue; }
            let dur_per_int = duration_per_day / day.len() as f32;
            let samples = (sample_rate as f32 * duration_per_day) as usize;

            for i in 0..samples {
                let t_sec = i as f32 / sample_rate as f32;
                let idx = ((t_sec / dur_per_int) as usize).min(day.len() - 1);
                let next = (idx + 1).min(day.len() - 1);
                let frac = (t_sec - idx as f32 * dur_per_int) / dur_per_int;
                let intensity = day[idx] + (day[next] - day[idx]) * frac;

                let wind_sample = wind_data[i % n_wind];
                let value = if intensity <= 1.0 { 0.0 } else { wind_sample * intensity / 15.0 };
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

        // Just mark 50% progress since we are about to encode
        let _ = prog_tx.send(super::Progress::Update { name: name.to_string(), fraction: 0.5 });

        let ffmpeg = shared::ffmpeg_bin();
        // Use -stats to allow run_ffmpeg to work (though we pass None frames, it just runs)
        let args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
            "-i".into(), tmp.to_string_lossy().to_string(),
            "-vn".into(), "-ar".into(), "44100".into(), "-ac".into(), "2".into(),
            "-b:a".into(), "192k".into(), "-codec:a".into(), "libmp3lame".into(),
            out.to_string_lossy().to_string(),
        ];
        
        shared::run_ffmpeg(&args, None, prog_tx, name)?;

        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&tmp_dir);
        Ok(())
    })
}
