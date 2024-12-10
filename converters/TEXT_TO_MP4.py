import cv2
import numpy as np
import PyPDF2 as p2
from PIL import ImageFont, ImageDraw, Image
import collections
import os 
import subprocess

def MP4_converter(li, Type, color, output_filename, font_path, ffmpeg_path, COLOR_PICKT, chunk_size=5):
    li = li.replace('Selected: ', '')

    color_map = {
    0: (255, 0, 0),     # Red
    1: (255, 127, 0),   # Orange
    2: (255, 255, 0),   # Yellow
    3: (0, 255, 0),     # Green
    4: (0, 255, 255),   # Cyan
    5: (255, 0, 255)    # Magenta
    }

    color = color.replace('#', '')
    color = tuple(int(color[i:i+2], 16) for i in (0, 2, 4))
    color = color[::-1]  # Reverse the tuple


    frame_height = 225
    frame_width = 600

    # Load font
    font = ImageFont.truetype(font_path, int(frame_height * 0.6))

    if Type == 'File':
        pdf_reader = p2.PdfFileReader(li)

        file = os.path.basename(li)
        name = file.replace('.pdf', '')

        if COLOR_PICKT == False:
            file_number = int(name)  # convert name to integer

            # remove first digits so 5001 becomes 001 and 4234 becomes 234
            file_number = file_number % 1000 - 1

            # calculate color index
            color_index = (file_number) % 6
            color = color_map[color_index]  # get the color from color_map
            # turn color RGB into BGR
            color = color[::-1]

        if os.path.exists(name+'.mp4'):
            return "copy"

        if os.path.exists(name+'_raw.mp4'):
            os.remove(name+'_raw.mp4')

        num_pages = pdf_reader.numPages
        chunks = []
        for i in range(num_pages):
            page = pdf_reader.getPage(i)
            text = page.extractText().replace('\n', ' ')
            chunks += [text[j:j + chunk_size] for j in range(0, len(text), chunk_size)]

    
        fourcc = cv2.VideoWriter_fourcc(*'mp4v')
        out = cv2.VideoWriter(name+"_raw"+".mp4", fourcc, 30.0, (frame_width, frame_height))

        # Calculate speed needed to have text scroll by in exactly 12 minutes
        total_width = sum(font.getsize(chunk)[0] for chunk in chunks)
        speed = 5

        # Initialize scroll position and active chunks
        scroll_position = frame_width  
        active_chunks = collections.deque()

        total_width = sum(font.getsize(chunk)[0] for chunk in chunks)

        # Calculate the total number of frames needed for all chunks to pass by at the given speed
        # Calculate the total frames
        total_frames = int(total_width / speed)

        # Add extra frames for the text to scroll off the screen
        total_frames += int(frame_width / speed) + 5

        while chunks and (not active_chunks or active_chunks[-1][1] + font.getsize(active_chunks[-1][0])[0] < frame_width):
            if active_chunks:
                chunk_position = active_chunks[-1][1] + font.getsize(active_chunks[-1][0])[0]
            else:
                chunk_position = scroll_position + frame_width  # start from right edge
            active_chunks.append((chunks.pop(0), chunk_position))

        for i in range(total_frames):
            frame = np.zeros((frame_height, frame_width, 3), np.uint8)
            frame_pil = Image.fromarray(frame)
            draw = ImageDraw.Draw(frame_pil)

            # Remove chunks that have moved out of frame on the left side
            while active_chunks and active_chunks[0][1] + font.getsize(active_chunks[0][0])[0] < scroll_position:
                active_chunks.popleft()

            # Add chunks that come into frame on the right side
            while chunks and (not active_chunks or active_chunks[-1][1] + font.getsize(active_chunks[-1][0])[0] <= scroll_position + frame_width):
                if active_chunks:
                    chunk_position = active_chunks[-1][1] + font.getsize(active_chunks[-1][0])[0]
                else:
                    chunk_position = scroll_position + frame_width  # start from right edge
                active_chunks.append((chunks.pop(0), chunk_position))

            # Draw all active chunks
            for chunk, chunk_position in active_chunks:
                x = chunk_position - scroll_position
                y = frame_height // 2 - font.getsize('A')[1] // 2
                draw.text((x, y), chunk, font=font, fill=color)

            frame = np.array(frame_pil)
            out.write(frame)

            # Scroll right
            scroll_position += speed

        out.release()

        input_file = name + "_raw" + ".mp4"
        output_file = name + ".mp4"

        # Use ffmpeg to convert the video
        command = [ffmpeg_path, '-i', input_file, '-c:v', 'libx264', '-pix_fmt', 'yuv420p', output_file]
        subprocess.run(command, check=True)

        # Remove the raw video file
        os.remove(input_file)

    else: 
        for file in os.listdir(li):
            if file.endswith(".pdf"):
                name = file.replace('.pdf', '')
                if COLOR_PICKT == False:
                    file_number = int(name)  # convert name to integer

                    # remove first digits so 5001 becomes 001 and 4234 becomes 234
                    file_number = file_number % 1000 - 1

                    # calculate color index
                    color_index = (file_number) % 6
                    color = color_map[color_index]  # get the color from color_map
                    # turn color RGB into BGR
                    color = color[::-1]

                file = li + '\\' + file
                
                if os.path.exists(name+'.mp4'):
                    continue

                if os.path.exists(name+'_raw.mp4'):
                    os.remove(name+'_raw.mp4')

                pdf_reader = p2.PdfFileReader(file)

                num_pages = pdf_reader.numPages
                chunks = []
                for i in range(num_pages):
                    page = pdf_reader.getPage(i)
                    text = page.extractText().replace('\n', ' ')
                    chunks += [text[j:j + chunk_size] for j in range(0, len(text), chunk_size)]

            
                fourcc = cv2.VideoWriter_fourcc(*'mp4v')
                out = cv2.VideoWriter(name+"_raw"+".mp4", fourcc, 30.0, (frame_width, frame_height))

                # Calculate speed needed to have text scroll by in exactly 12 minutes
                total_width = sum(font.getsize(chunk)[0] for chunk in chunks)
                speed = 5

                # Initialize scroll position and active chunks
                scroll_position = frame_width  
                active_chunks = collections.deque()

                total_width = sum(font.getsize(chunk)[0] for chunk in chunks)

                # Calculate the total number of frames needed for all chunks to pass by at the given speed
                # Calculate the total frames
                total_frames = int(total_width / speed)

                # Add extra frames for the text to scroll off the screen
                total_frames += int(frame_width / speed) + 5

                while chunks and (not active_chunks or active_chunks[-1][1] + font.getsize(active_chunks[-1][0])[0] < frame_width):
                    if active_chunks:
                        chunk_position = active_chunks[-1][1] + font.getsize(active_chunks[-1][0])[0]
                    else:
                        chunk_position = scroll_position + frame_width  # start from right edge
                    active_chunks.append((chunks.pop(0), chunk_position))

                for i in range(total_frames):
                    frame = np.zeros((frame_height, frame_width, 3), np.uint8)
                    frame_pil = Image.fromarray(frame)
                    draw = ImageDraw.Draw(frame_pil)

                    # Remove chunks that have moved out of frame on the left side
                    while active_chunks and active_chunks[0][1] + font.getsize(active_chunks[0][0])[0] < scroll_position:
                        active_chunks.popleft()

                    # Add chunks that come into frame on the right side
                    while chunks and (not active_chunks or active_chunks[-1][1] + font.getsize(active_chunks[-1][0])[0] <= scroll_position + frame_width):
                        if active_chunks:
                            chunk_position = active_chunks[-1][1] + font.getsize(active_chunks[-1][0])[0]
                        else:
                            chunk_position = scroll_position + frame_width  # start from right edge
                        active_chunks.append((chunks.pop(0), chunk_position))

                    # Draw all active chunks
                    for chunk, chunk_position in active_chunks:
                        x = chunk_position - scroll_position
                        y = frame_height // 2 - font.getsize('A')[1] // 2
                        draw.text((x, y), chunk, font=font, fill=color)

                    frame = np.array(frame_pil)
                    out.write(frame)

                    # Scroll right
                    scroll_position += speed

                out.release()

                input_file = name + "_raw" + ".mp4"
                output_file = name + ".mp4"

                # Use ffmpeg to convert the video
                command = [ffmpeg_path, '-i', input_file, '-c:v', 'libx264', '-pix_fmt', 'yuv420p', output_file]
                subprocess.run(command, check=True)

                # Remove the raw video file
                os.remove(input_file)