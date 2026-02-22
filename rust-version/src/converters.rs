use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use image::{imageops, GenericImage, GenericImageView, RgbImage};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};

// ─── shared helpers ───────────────────────────────────────────────────────────

fn run_ffmpeg(args: &[&str]) -> Result<(), String> {
    let ok = Command::new("ffmpeg")
        .args(args)
        .status()
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?
        .success();
    ok.then_some(()).ok_or_else(|| "ffmpeg exited with error".to_string())
}

fn extract_text(pdf_path: &Path) -> Result<String, String> {
    let bytes = fs::read(pdf_path).map_err(|e| e.to_string())?;
    pdf_extract::extract_text_from_mem(&bytes).map_err(|e| e.to_string())
}

/// Walk a single file or every PDF in a folder and call `f` on each.
fn each_pdf<F>(path: &Path, is_folder: bool, mut f: F) -> Result<(), String>
where
    F: FnMut(&Path, &str) -> Result<(), String>,
{
    if is_folder {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let p = entry.map_err(|e| e.to_string())?.path();
            if p.extension().and_then(|e| e.to_str()) == Some("pdf") {
                let stem = p.file_stem().unwrap().to_string_lossy().to_string();
                f(&p, &stem)?;
            }
        }
    } else {
        if path.extension().and_then(|e| e.to_str()) != Some("pdf") {
            return Err("Selected file is not a PDF".into());
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        f(path, &stem)?;
    }
    Ok(())
}

// ─── WIND ─────────────────────────────────────────────────────────────────────

pub fn convert_wind(file_path: &Path, is_folder: bool) -> Result<(), String> {
    each_pdf(file_path, is_folder, |pdf, name| {
        let out = pdf.with_file_name(format!("{name}.mp3"));
        if out.exists() {
            return Ok(());
        }

        let text = extract_text(pdf)?;
        let mut wind_intensities: Vec<Vec<f32>> = Vec::new();

        for line in text.split('\n') {
            let day: Vec<f32> = line
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect();
            if !day.is_empty() {
                wind_intensities.push(day);
            }
        }

        if wind_intensities.len() >= 25 {
            wind_intensities.truncate(24);
        }

        let wind_path = Path::new("assets/Wind_Loop.wav");
        if !wind_path.exists() {
            return Err("assets/Wind_Loop.wav not found".into());
        }

        let mut reader = hound::WavReader::open(wind_path).map_err(|e| e.to_string())?;
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
            if day.is_empty() {
                continue;
            }
            let dur_per_int = duration_per_day / day.len() as f32;
            let samples = (sample_rate as f32 * duration_per_day) as usize;

            for i in 0..samples {
                let t_sec = i as f32 / sample_rate as f32;
                let idx = ((t_sec / dur_per_int) as usize).min(day.len() - 1);
                let next = (idx + 1).min(day.len() - 1);
                let frac = (t_sec - idx as f32 * dur_per_int) / dur_per_int;
                let intensity = day[idx] + (day[next] - day[idx]) * frac;

                let wind_sample = wind_data[i % n_wind];
                let value = if intensity <= 1.0 {
                    0.0
                } else {
                    wind_sample * intensity / 15.0
                };
                // +3 dB ≈ ×1.412
                output.push(value * 1.412);
            }
        }

        let tmp = pdf.with_file_name(format!("{name}_tmp.wav"));
        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&tmp, spec).map_err(|e| e.to_string())?;
            for s in &output {
                w.write_sample(*s).map_err(|e| e.to_string())?;
            }
            w.finalize().map_err(|e| e.to_string())?;
        }

        run_ffmpeg(&[
            "-y",
            "-i",
            tmp.to_str().unwrap(),
            "-vn",
            "-ar",
            "44100",
            "-ac",
            "2",
            "-b:a",
            "192k",
            out.to_str().unwrap(),
        ])?;
        let _ = fs::remove_file(tmp);
        Ok(())
    })
}

// ─── BPM ──────────────────────────────────────────────────────────────────────

pub fn convert_bpm(file_path: &Path, is_folder: bool) -> Result<(), String> {
    each_pdf(file_path, is_folder, |pdf, name| {
        let out = pdf.with_file_name(format!("{name}.mp3"));
        if out.exists() {
            return Ok(());
        }

        let text = extract_text(pdf)?;
        let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();

        let mut bpm_list: Vec<u32> = Vec::new();
        for chunk in digits.as_bytes().chunks(3) {
            if let Ok(n) = std::str::from_utf8(chunk).unwrap_or("").parse::<u32>() {
                bpm_list.push(n);
            }
        }
        if bpm_list.is_empty() {
            return Ok(());
        }

        let framerate = 3000u32;
        let amplitude = 32000i16;
        let total_duration_secs = 12.0 * 60.0f64;
        let pulse_duration = (total_duration_secs / bpm_list.len() as f64) * 1.0255;
        let twelve_min_samples = (total_duration_secs * framerate as f64) as usize;

        let tmp = pdf.with_file_name(format!("{name}_tmp.wav"));
        {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: framerate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut w = hound::WavWriter::create(&tmp, spec).map_err(|e| e.to_string())?;
            let mut written = 0usize;

            let mut write_n = |w: &mut hound::WavWriter<_>,
                               sample: i16,
                               n: usize,
                               written: &mut usize|
             -> Result<(), String> {
                for _ in 0..n {
                    if *written >= twelve_min_samples {
                        break;
                    }
                    w.write_sample(sample).map_err(|e| e.to_string())?;
                    w.write_sample(sample).map_err(|e| e.to_string())?;
                    *written += 1;
                }
                Ok(())
            };

            for &bpm in &bpm_list {
                if bpm == 0 || written >= twelve_min_samples {
                    break;
                }
                let pulses = ((bpm as f64 / 60.0) * pulse_duration) as usize;
                let pulse_len = ((60.0 / bpm as f64 * 2.0) * framerate as f64) as usize;
                let boosted = (amplitude as f64 * 3.162)
                    .clamp(i16::MIN as f64, i16::MAX as f64) as i16;

                for _ in 0..pulses {
                    write_n(&mut w, 0, 1, &mut written)?;
                    write_n(&mut w, boosted, pulse_len, &mut written)?;
                }
            }

            // pad to 12 minutes
            let last_bpm = *bpm_list.last().unwrap();
            let pulse_len = ((60.0 / last_bpm as f64 * 2.0) * framerate as f64) as usize;
            let boosted = (amplitude as f64 * 3.162)
                .clamp(i16::MIN as f64, i16::MAX as f64) as i16;
            while written < twelve_min_samples {
                write_n(&mut w, boosted, pulse_len, &mut written)?;
            }

            w.finalize().map_err(|e| e.to_string())?;
        }

        run_ffmpeg(&[
            "-y",
            "-i",
            tmp.to_str().unwrap(),
            "-vn",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-b:a",
            "320k",
            out.to_str().unwrap(),
        ])?;
        let _ = fs::remove_file(tmp);
        Ok(())
    })
}

// ─── CLOUDS ───────────────────────────────────────────────────────────────────

pub fn convert_clouds(file_path: &Path, is_folder: bool) -> Result<(), String> {
    each_pdf(file_path, is_folder, |pdf, name| {
        let out = pdf.with_file_name(format!("{name}.mp4"));
        if out.exists() {
            return Ok(());
        }

        let doc = lopdf::Document::load(pdf).map_err(|e| e.to_string())?;
        let mut frames: Vec<RgbImage> = Vec::new();

        for &page_id in doc.page_iter() {
            if let Ok(lopdf::Object::Dictionary(d)) = doc.get_object(page_id) {
                if let Ok(res) = d.get(b"Resources") {
                    if let (Ok(xobjs_obj)) = res.as_dict().and_then(|r| r.get(b"XObject")) {
                        if let Ok(xobjs) = xobjs_obj.as_dict() {
                            for (_, v) in xobjs.iter() {
                                if let Ok(ref_id) = v.as_reference() {
                                    if let Ok(lopdf::Object::Stream(s)) =
                                        doc.get_object(ref_id)
                                    {
                                        if let Ok(content) = s.decompressed_content() {
                                            if let Ok(img) = image::load_from_memory(&content) {
                                                let rgb = imageops::resize(
                                                    &img.to_rgb8(),
                                                    750,
                                                    360,
                                                    imageops::FilterType::Triangle,
                                                );
                                                frames.push(rgb);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if frames.is_empty() {
            return Err("No images found in PDF".into());
        }

        let (w, h) = (750u32, 360u32);
        let total_w = w * frames.len() as u32 + w;
        let mut strip = RgbImage::new(total_w, h);
        for (i, img) in frames.iter().enumerate() {
            imageops::replace(&mut strip, img, (i as i64) * w as i64, 0);
        }

        let fps = 25.0f64;
        let video_dur = 12.0 * 60.0;
        let total_frames = (fps * video_dur) as usize;
        let scroll = (total_w as f64 - w as f64) / total_frames as f64;

        let mut child = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "rawvideo",
                "-pix_fmt", "rgb24",
                "-s", &format!("{w}x{h}"),
                "-r", &fps.to_string(),
                "-i", "pipe:0",
                "-c:v", "libx264",
                "-pix_fmt", "yuv420p",
                out.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;

        {
            let stdin = child.stdin.as_mut().unwrap();
            for fi in 0..total_frames {
                let x = ((fi as f64 * scroll) as u32).min(total_w - w);
                let view = strip.view(x, 0, w, h);
                let raw: Vec<u8> = (0..h)
                    .flat_map(|y| {
                        (0..w).flat_map(move |x2| {
                            let p = view.get_pixel(x2, y);
                            [p[0], p[1], p[2]]
                        })
                    })
                    .collect();
                stdin.write_all(&raw).map_err(|e| e.to_string())?;
            }
        }
        child.wait().map_err(|e| e.to_string())?;
        Ok(())
    })
}

// ─── RGB ──────────────────────────────────────────────────────────────────────

fn lerp_color(a: [u8; 3], b: [u8; 3], steps: usize) -> Vec<[u8; 3]> {
    (0..steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            [
                (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
                (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
                (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
            ]
        })
        .collect()
}

pub fn convert_rgb(file_path: &Path, is_folder: bool) -> Result<(), String> {
    each_pdf(file_path, is_folder, |pdf, name| {
        let out = pdf.with_file_name(format!("{name}.mp4"));
        if out.exists() {
            return Ok(());
        }

        let text = extract_text(pdf)?;
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
        if colors.is_empty() {
            return Err("No colors found in PDF".into());
        }

        let mut interpolated: Vec<[u8; 3]> = Vec::new();
        for w in colors.windows(2) {
            interpolated.extend(lerp_color(w[0], w[1], 3000));
        }

        let num_frames = 25 * 720;
        let gradient: Vec<[u8; 3]> = (0..num_frames)
            .map(|i| {
                let idx = (i * interpolated.len()) / num_frames;
                interpolated[idx.min(interpolated.len() - 1)]
            })
            .collect();

        let mut child = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "rawvideo",
                "-pix_fmt", "rgb24",
                "-s", "520x520",
                "-r", "25",
                "-i", "pipe:0",
                "-c:v", "libx264",
                "-pix_fmt", "yuv420p",
                out.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;

        {
            let stdin = child.stdin.as_mut().unwrap();
            let mut raw = vec![0u8; 520 * 520 * 3];
            for color in &gradient {
                for px in raw.chunks_mut(3) {
                    px[0] = color[0];
                    px[1] = color[1];
                    px[2] = color[2];
                }
                stdin.write_all(&raw).map_err(|e| e.to_string())?;
            }
        }
        child.wait().map_err(|e| e.to_string())?;
        Ok(())
    })
}

// ─── TEXT ─────────────────────────────────────────────────────────────────────

pub fn convert_text(file_path: &Path, is_folder: bool, color: [u8; 3]) -> Result<(), String> {
    let font_path = Path::new("assets/JdLcdRoundedRegular-vXwE.ttf");
    if !font_path.exists() {
        return Err("Font file assets/JdLcdRoundedRegular-vXwE.ttf not found".into());
    }
    let font_data = fs::read(font_path).map_err(|e| e.to_string())?;

    each_pdf(file_path, is_folder, |pdf, name| {
        let out = pdf.with_file_name(format!("{name}.mp4"));
        if out.exists() {
            return Ok(());
        }

        let text = extract_text(pdf)?;
        let text = text.replace('\n', " ");
        let chunk_size = 5;
        let mut chunks: Vec<String> = text
            .chars()
            .collect::<Vec<_>>()
            .chunks(chunk_size)
            .map(|c| c.iter().collect())
            .collect();

        let frame_w = 600u32;
        let frame_h = 225u32;
        let scale = Scale::uniform(frame_h as f32 * 0.6);
        let speed = 5.0f32;

        let font = Font::try_from_vec(font_data.clone()).ok_or("Failed to load font")?;

        let measure = |s: &str| -> f32 {
            let mut w = 0.0f32;
            let mut last = None;
            for ch in s.chars() {
                let g = font.glyph(ch);
                if let Some(prev) = last {
                    w += font.pair_kerning(scale, prev, g.id());
                }
                w += g.scaled(scale).h_metrics().advance_width;
                last = Some(g.id());
            }
            w
        };

        let total_text_w: f32 = chunks.iter().map(|c| measure(c)).sum();
        let total_frames =
            (total_text_w / speed) as usize + (frame_w as f32 / speed) as usize + 5;

        let mut child = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "rawvideo",
                "-pix_fmt", "rgb24",
                "-s", &format!("{frame_w}x{frame_h}"),
                "-r", "30",
                "-i", "pipe:0",
                "-c:v", "libx264",
                "-pix_fmt", "yuv420p",
                out.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;

        let text_color = image::Rgb(color);
        let mut scroll = frame_w as f32;
        let mut active: VecDeque<(String, f32)> = VecDeque::new();

        {
            let stdin = child.stdin.as_mut().unwrap();

            for _ in 0..total_frames {
                // evict chunks scrolled off left edge
                while let Some((c, pos)) = active.front() {
                    if *pos + measure(c) < scroll {
                        active.pop_front();
                    } else {
                        break;
                    }
                }
                // enqueue new chunks that come into view from right
                while !chunks.is_empty() {
                    let next_x = active
                        .back()
                        .map(|(c, p)| p + measure(c))
                        .unwrap_or(scroll + frame_w as f32);
                    if next_x <= scroll + frame_w as f32 {
                        active.push_back((chunks.remove(0), next_x));
                    } else {
                        break;
                    }
                }

                let mut img = RgbImage::new(frame_w, frame_h);
                for (chunk, pos) in &active {
                    let x = (*pos - scroll) as i32;
                    let y = (frame_h as f32 / 2.0 - scale.y / 2.0) as i32;
                    draw_text_mut(&mut img, text_color, x, y, scale, &font, chunk);
                }

                stdin.write_all(img.as_raw()).map_err(|e| e.to_string())?;
                scroll += speed;
            }
        }

        child.wait().map_err(|e| e.to_string())?;
        Ok(())
    })
}
