use std::io::Write;
use std::path::Path;
use super::{shared, CancelFlag, ProgressTx};

fn lerp_color(a: [u8; 3], b: [u8; 3], steps: usize) -> Vec<[u8; 3]> {
    (0..steps).map(|i| {
        let t = i as f32 / steps as f32;
        [
            (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
            (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
            (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
        ]
    }).collect()
}

pub fn convert_rgb(
    file_path: &Path,
    is_folder: bool,
    tx: ProgressTx,
    cancel: CancelFlag,
) -> Result<(), String> {
    shared::process_files(file_path, is_folder, tx, cancel.clone(), |pdf, name, prog_tx| {
        let out = pdf.with_file_name(format!("{name}.mp4"));
        let partial_out = out.with_extension("tmp.mp4");
        if out.exists() {
            return Ok(());
        }

        let text = shared::extract_text(pdf)?;
        let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
        let mut colors: Vec<[u8; 3]> = Vec::new();
        for chunk in digits.as_bytes().chunks(9) {
            if chunk.len() == 9 {
                let s = std::str::from_utf8(chunk).unwrap_or("");
                let r = s[0..3].parse::<u8>().unwrap_or(0);
                let g = s[3..6].parse::<u8>().unwrap_or(0);
                let b = s[6..9].parse::<u8>().unwrap_or(0);
                colors.push([r, g, b]);
            }
        }
        if colors.is_empty() { return Err("No RGB color data found".into()); }

        let mut interpolated: Vec<[u8; 3]> = Vec::new();
        for w in colors.windows(2) {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("Cancelled.".into());
            }
            interpolated.extend(lerp_color(w[0], w[1], 3000));
        }

        let num_frames = 25 * 720;
        let gradient: Vec<[u8; 3]> = (0..num_frames).map(|i| {
            let idx = (i * interpolated.len()) / num_frames;
            interpolated[idx.min(interpolated.len() - 1)]
        }).collect();

        let mut args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-f".into(), "rawvideo".into(), "-pix_fmt".into(), "rgb24".into(),
            "-s".into(), "520x520".into(), "-r".into(), "25".into(),
            "-i".into(), "pipe:0".into(), "-c:v".into(), "libx264".into(),
            "-preset".into(), shared::ffmpeg_preset(), "-pix_fmt".into(), "yuv420p".into(),
        ];
        
        if is_folder {
            args.push("-threads".into());
            args.push("2".into());
        }
        
        args.push(partial_out.to_string_lossy().to_string());

        let result = shared::run_ffmpeg_stream(&args, prog_tx, name, cancel.clone(), |stdin| {
            let mut raw = vec![0u8; 520 * 520 * 3];
            let mut count = 0;
            for color in &gradient {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("Cancelled.".into());
                }

                for px in raw.chunks_mut(3) {
                    px[0] = color[0]; px[1] = color[1]; px[2] = color[2];
                }
                if stdin.write_all(&raw).is_err() { break; } 
                
                count += 1;
                if count % 250 == 0 {
                    let _ = prog_tx.send(super::Progress::Update {
                        name: name.to_string(),
                        fraction: count as f32 / num_frames as f32,
                    });
                }
            }
            Ok(())
        });
        
        if result.is_ok() && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = std::fs::rename(&partial_out, &out);
        } else {
            let _ = std::fs::remove_file(&partial_out);
        }

        result
    })
}
