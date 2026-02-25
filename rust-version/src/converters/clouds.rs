use std::fs;
use std::path::{Path, PathBuf};
use image::{imageops};
use std::io::Write;
use super::{shared, CancelFlag, ProgressTx};

fn list_images(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            ext == "png" || ext == "jpg" || ext == "jpeg"
        })
        .collect();
    files.sort();
    Ok(files)
}

pub fn convert_clouds(
    file_path: &Path,
    is_folder: bool,
    stitch_images: bool, 
    tx: ProgressTx,
    cancel: CancelFlag,
) -> Result<(), String> {
    if is_folder && stitch_images {
        let out = file_path.with_file_name(format!("{}_clouds.mp4", file_path.file_name().unwrap_or_default().to_string_lossy()));
        let partial_out = out.with_extension("tmp.mp4");
        if out.exists() { return Ok(()); }

        let _ = tx.send(super::Progress::Init { total: 1 });
        let stem = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let _ = tx.send(super::Progress::Start { name: stem.clone() });

        let page_files = list_images(file_path)?;
        if page_files.is_empty() {
            return Err("No PNG/JPG images found in the selected folder".into());
        }

        let mut images = Vec::new();
        // Add black image at the start
        images.push(image::RgbImage::new(750, 360));
        
        for p in &page_files {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("Cancelled.".into());
            }
            let img = image::open(p).map_err(|e| format!("open {}: {e}", p.display()))?.to_rgb8();
            let resized = imageops::resize(&img, 750, 360, imageops::FilterType::Triangle);
            images.push(resized);
        }

        // Add black image at the end
        images.push(image::RgbImage::new(750, 360));

        let video_dur = 12.0 * 60.0;
        let fps = 25.0;
        let total_frames = (video_dur * fps) as usize;
        
        let args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-f".into(), "rawvideo".into(), "-pix_fmt".into(), "rgb24".into(),
            "-s".into(), "750x360".into(), "-r".into(), fps.to_string(),
            "-i".into(), "pipe:0".into(), "-c:v".into(), "libx264".into(),
            "-preset".into(), shared::ffmpeg_preset(), "-pix_fmt".into(), "yuv420p".into(),
            partial_out.to_string_lossy().to_string(),
        ];
        
        let total_virtual_w = images.len() as f32 * 750.0;

        let result = shared::run_ffmpeg_stream(&args, &tx, &stem, cancel.clone(), |stdin| {
            let mut frame = vec![0u8; 750 * 360 * 3];
            for f in 0..total_frames {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) { return Err("Cancelled.".into()); }
                
                let progress = f as f32 / (total_frames as f32 - 1.0).max(1.0);
                let x_offset = progress * (total_virtual_w - 750.0).max(0.0);
                let img_idx1 = (x_offset / 750.0).floor() as usize;
                let local_x = (x_offset % 750.0).round() as u32;

                if img_idx1 >= images.len() - 1 {
                    let img = &images[images.len() - 1];
                    frame.copy_from_slice(img.as_raw());
                } else {
                    let img1 = &images[img_idx1];
                    let img2 = &images[img_idx1 + 1];
                    for y in 0..360 {
                        let w1 = 750 - local_x;
                        let w2 = local_x;
                        
                        let src1_s = (y * 750 + local_x) as usize * 3;
                        let src1_e = (y * 750 + 750) as usize * 3;
                        let dst1_s = (y * 750) as usize * 3;
                        let dst1_e = (y * 750 + w1) as usize * 3;
                        frame[dst1_s..dst1_e].copy_from_slice(&img1.as_raw()[src1_s..src1_e]);

                        if w2 > 0 {
                            let src2_s = (y * 750) as usize * 3;
                            let src2_e = (y * 750 + w2) as usize * 3;
                            let dst2_s = (y * 750 + w1) as usize * 3;
                            let dst2_e = (y * 750 + 750) as usize * 3;
                            frame[dst2_s..dst2_e].copy_from_slice(&img2.as_raw()[src2_s..src2_e]);
                        }
                    }
                }

                if stdin.write_all(&frame).is_err() { break; }
                
                if f % 250 == 0 {
                    let _ = tx.send(super::Progress::Update {
                        name: stem.clone(),
                        fraction: f as f32 / total_frames as f32,
                    });
                }
            }
            Ok(())
        });

        if result.is_ok() && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = tx.send(super::Progress::Done { name: stem });
            let _ = fs::rename(&partial_out, &out);
        } else {
            let _ = fs::remove_file(&partial_out);
        }
        result
    } else {
        shared::process_files(file_path, is_folder, tx, cancel.clone(), |pdf, name, prog_tx| {
            let out = pdf.with_file_name(format!("{name}.mp4"));
            let partial_out = out.with_extension("tmp.mp4");
            if out.exists() {
                return Ok(());
            }

            let tmp_dir = shared::make_temp_dir("clouds")?;
            let pages_dir = tmp_dir.join("pages");
            fs::create_dir_all(&pages_dir).map_err(|e| e.to_string())?;

            let pdftoppm = shared::pdftoppm_bin();
            let dpi = shared::pdf_render_dpi();
            let prefix = pages_dir.join("page");
            let args: Vec<String> = vec![
                "-r".into(), dpi.to_string(), "-png".into(),
                pdf.to_string_lossy().to_string(), prefix.to_string_lossy().to_string(),
            ];
            shared::run_cmd(&pdftoppm, &args)?;

            let page_files = list_images(&pages_dir)?; 
            if page_files.is_empty() {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Err("pdftoppm produced no PNGs".into());
            }

            let mut images = Vec::new();
            // Add black image at the start
            images.push(image::RgbImage::new(750, 360));

            for p in &page_files {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = fs::remove_dir_all(&tmp_dir);
                    return Err("Cancelled.".into());
                }
                let img = image::open(p).map_err(|e| format!("open {}: {e}", p.display()))?.to_rgb8();
                let resized = imageops::resize(&img, 750, 360, imageops::FilterType::Triangle);
                images.push(resized);
            }

            // Add black image at the end
            images.push(image::RgbImage::new(750, 360));

            let video_dur = 12.0 * 60.0;
            let fps = 25.0;
            let total_frames = (video_dur * fps) as usize;
            
            let mut args: Vec<String> = vec![
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-f".into(), "rawvideo".into(), "-pix_fmt".into(), "rgb24".into(),
                "-s".into(), "750x360".into(), "-r".into(), fps.to_string(),
                "-i".into(), "pipe:0".into(), "-c:v".into(), "libx264".into(),
                "-preset".into(), shared::ffmpeg_preset(), "-pix_fmt".into(), "yuv420p".into(),
            ];
            
            if is_folder {
                args.push("-threads".into());
                args.push("2".into());
            }
            args.push(partial_out.to_string_lossy().to_string());
            
            let total_virtual_w = images.len() as f32 * 750.0;

            let result = shared::run_ffmpeg_stream(&args, prog_tx, name, cancel.clone(), |stdin| {
                let mut frame = vec![0u8; 750 * 360 * 3];
                for f in 0..total_frames {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) { return Err("Cancelled.".into()); }
                    
                    let progress = f as f32 / (total_frames as f32 - 1.0).max(1.0);
                    let x_offset = progress * (total_virtual_w - 750.0).max(0.0);
                    let img_idx1 = (x_offset / 750.0).floor() as usize;
                    let local_x = (x_offset % 750.0).round() as u32;

                    if img_idx1 >= images.len() - 1 {
                        let img = &images[images.len() - 1];
                        frame.copy_from_slice(img.as_raw());
                    } else {
                        let img1 = &images[img_idx1];
                        let img2 = &images[img_idx1 + 1];
                        for y in 0..360 {
                            let w1 = 750 - local_x;
                            let w2 = local_x;
                            
                            let src1_s = (y * 750 + local_x) as usize * 3;
                            let src1_e = (y * 750 + 750) as usize * 3;
                            let dst1_s = (y * 750) as usize * 3;
                            let dst1_e = (y * 750 + w1) as usize * 3;
                            frame[dst1_s..dst1_e].copy_from_slice(&img1.as_raw()[src1_s..src1_e]);

                            if w2 > 0 {
                                let src2_s = (y * 750) as usize * 3;
                                let src2_e = (y * 750 + w2) as usize * 3;
                                let dst2_s = (y * 750 + w1) as usize * 3;
                                let dst2_e = (y * 750 + 750) as usize * 3;
                                frame[dst2_s..dst2_e].copy_from_slice(&img2.as_raw()[src2_s..src2_e]);
                            }
                        }
                    }

                    if stdin.write_all(&frame).is_err() { break; }
                    
                    if f % 250 == 0 {
                        let _ = prog_tx.send(super::Progress::Update {
                            name: name.to_string(),
                            fraction: f as f32 / total_frames as f32,
                        });
                    }
                }
                Ok(())
            });

            let _ = fs::remove_dir_all(&tmp_dir);
            
            if result.is_ok() && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = fs::rename(&partial_out, &out);
            } else {
                let _ = fs::remove_file(&partial_out);
            }

            result
        })
    }
}