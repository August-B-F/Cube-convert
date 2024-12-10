import numpy as np
import PyPDF2 as p2
import os
import cv2
import subprocess

def interpolate_colors(color1, color2, steps):
    r_step = (color2[0] - color1[0]) / steps
    g_step = (color2[1] - color1[1]) / steps
    b_step = (color2[2] - color1[2]) / steps

    return [(color1[0] + r_step * step, color1[1] + g_step * step, color1[2] + b_step * step) for step in range(steps)]

def RGB_converter(List, Type, output_filename, ffmpeg_path):
    List = List.replace('Selected: ', '')
    gradient = []
    colors = []

    if Type == 'File':
        pdf_reader = p2.PdfFileReader(List)
        num_pages = pdf_reader.numPages

        file = List.split('\\')
        file = file[len(file)-1]
        name = file.replace('.pdf', '')

        if os.path.exists(name+'.mp4'):
            return "copy"
        
        if os.path.exists(name+'_raw.mp4'):
            os.remove(name+'_raw.mp4')

        for i in range(num_pages):
            page = pdf_reader.getPage(i)
            text = page.extractText()
            text = text.split('\n')

            for i in range(len(text)):
                text[i] = text[i][1:]
                text[i] = text[i][1:]

            text = ''.join(text)

            new_text = ''
            for char in text:
                if char.isnumeric():
                    new_text += char
            text = new_text

            for n in range(int(len(text)/3)):
                colors.append(int(text[n*3:n*3+3]))

            colors = [tuple(colors[i:i+3]) for i in range(0, len(colors), 3)]

        if colors == []:
            x = {}
            for i in range(1000000):
                x = {1: x}
            repr(x)

        interpolated_colors = []
        for i in range(len(colors)-1):
            interpolated_colors.extend(interpolate_colors(colors[i], colors[i+1], 3000))  # Increased to 3000

        num_frames = 25 * 720  # 12 minutes at 25 FPS
        gradient = [interpolated_colors[i*len(interpolated_colors)//num_frames] for i in range(num_frames)]

        fourcc = cv2.VideoWriter_fourcc(*'mp4v')
        video = cv2.VideoWriter(name+'_raw.mp4', fourcc, 25, (520, 520))

        for color in gradient:
            frame = np.ones((520, 520, 3)) * np.array(color[::-1], dtype=np.uint8)  # Reverse color order for BGR
            video.write(np.uint8(frame))

        video.release()

        input_file = name + "_raw" + ".mp4"
        output_file = name + ".mp4"

        # Use ffmpeg to convert the video
        command = [ffmpeg_path, '-i', input_file, '-c:v', 'libx264', '-pix_fmt', 'yuv420p', output_file]
        subprocess.run(command, check=True)

        # Remove the raw video file
        os.remove(input_file)

    else:
        for file in os.listdir(List):
            if file.endswith(".pdf"):

                gradient =[]
                colors = []

                name = file.replace('.pdf', '')
                print(file)

                if os.path.exists(name+'.mp4'):
                    continue

                if os.path.exists(name+'_raw.mp4'):
                    os.remove(name+'_raw.mp4')

                pdf_reader = p2.PdfFileReader(List+'\\'+file)
                num_pages = pdf_reader.numPages

                for i in range(num_pages):
                    page = pdf_reader.getPage(i)
                    text = page.extractText()
                    text = text.split('\n')

                    for i in range(len(text)):
                        text[i] = text[i][1:]
                        text[i] = text[i][1:]

                    text = ''.join(text)

                    new_text = ''
                    for char in text:
                        if char.isnumeric():
                            new_text += char
                    text = new_text

                    for n in range(int(len(text)/3)):
                        colors.append(int(text[n*3:n*3+3]))

                    colors = [tuple(colors[i:i+3]) for i in range(0, len(colors), 3)]

                if colors == []:
                    x = {}
                    for i in range(1000000):
                        x = {1: x}
                    repr(x)

                interpolated_colors = []
                for i in range(len(colors)-1):
                    interpolated_colors.extend(interpolate_colors(colors[i], colors[i+1], 3000))  # Increased to 3000

                num_frames = 25 * 720  # 12 minutes at 25 FPS
                gradient = [interpolated_colors[i*len(interpolated_colors)//num_frames] for i in range(num_frames)]

                fourcc = cv2.VideoWriter_fourcc(*'mp4v')
                video = cv2.VideoWriter(name+'_raw.mp4', fourcc, 25, (520, 520))

                for color in gradient:
                    frame = np.ones((520, 520, 3)) * np.array(color[::-1], dtype=np.uint8)  # Reverse color order for BGR
                    video.write(np.uint8(frame))
                    
                video.release()

                input_file = name + "_raw" + ".mp4"
                output_file = name + ".mp4"

                # Use ffmpeg to convert the video
                command = [ffmpeg_path, '-i', input_file, '-c:v', 'libx264', '-pix_fmt', 'yuv420p', output_file]
                subprocess.run(command, check=True)

                # Remove the raw video file
                os.remove(input_file)