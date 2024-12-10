import numpy as np
import fitz
import cv2
import os
from PIL import Image 
import subprocess

class VideoCreator:
    def __init__(self, image, video_file, total_frames, scroll_speed, video_width, video_height, fps):
        self.image = image
        self.video_file = video_file
        self.total_frames = total_frames
        self.scroll_speed = scroll_speed
        self.video_width = video_width
        self.video_height = video_height
        self.fps = fps

    def create_video(self):
        fourcc = cv2.VideoWriter_fourcc(*'mp4v')
        video_writer = cv2.VideoWriter(self.video_file, fourcc, self.fps, (self.video_width, self.video_height))

        print(self.scroll_speed)

        for frame_idx in range(self.total_frames):
            x_offset = int(frame_idx * self.scroll_speed) 
            cropped_image = self.image[:, x_offset:x_offset + self.video_width]

            resized_image = cv2.resize(cropped_image, (self.video_width, self.video_height))

            video_writer.write(resized_image)

        video_writer.release()
        
def CLOUDS_converter(li, Type, ffmpeg_path):
    li = li.replace('Selected: ', '')

    if Type == 'File':
        # Open the PDF file
        pdf_file = fitz.open(li)

        # Extract filename from path
        file = os.path.basename(li)
        name = file.replace('.pdf', '')

        if os.path.exists(name+'.mp4'):
            return "copy"
        
        #if raw video exists, delete it
        if os.path.exists(name+'_raw.mp4'):
            os.remove(name+'_raw.mp4')

        # Initialize an empty list to store the images
        images = []

        # Process each page in the PDF file
        for pg in range(pdf_file.page_count):
            page = pdf_file[pg]
            zoom = 2
            trans = fitz.Matrix(zoom, zoom)
            pm = page.get_pixmap(matrix=trans, alpha=False)

            # Convert the pixmap to a PIL Image object and then to a numpy array
            img = np.array(Image.frombytes("RGB", [pm.width, pm.height], pm.samples))
            img = cv2.resize(img, (750, 360))

            images.append(img)

        # Combine the images horizontally
        image = np.hstack(images)

        # add black image to the end of the image
        black_image = np.zeros((image.shape[0], 750, 3), dtype=np.uint8)
        image = np.hstack((image, black_image))

        # Video settings
        video_duration = 12 * 60  # 12 minutes
        fps = 25
        video_width = 750
        video_height = 360
        video_file = name+"_raw" + ".mp4"

        # Calculate the scrolling speed
        scroll_speed = (image.shape[1] - video_width) / (video_duration * fps)
        # Calculate the total frames
        total_frames = video_duration * fps

        # Create the video
        creator = VideoCreator(image, video_file, total_frames, scroll_speed, video_width, video_height, fps)
        creator.create_video()

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
                file = li + '\\' + file
                
                if os.path.exists(name+'.mp4'):
                    continue

                if os.path.exists(name+'_raw.mp4'):
                    os.remove(name+'_raw.mp4')

                pdf_file = fitz.open(file)
                print('Processing: ' + file)

                # Initialize an empty list to store the images
                images = []

                # Process each page in the PDF file
                for pg in range(pdf_file.page_count):
                    page = pdf_file[pg]
                    zoom = 2
                    trans = fitz.Matrix(zoom, zoom)
                    pm = page.get_pixmap(matrix=trans, alpha=False)

                    # Convert the pixmap to a PIL Image object and then to a numpy array
                    img = np.array(Image.frombytes("RGB", [pm.width, pm.height], pm.samples))
                    img = cv2.resize(img, (750, 360))

                    images.append(img)

                # Combine the images horizontally
                image = np.hstack(images)

                # Video settings
                video_duration = 12 * 60  # 12 minutes
                fps = 25
                video_width = 750
                video_height = 360
                video_file = name+"_raw" + ".mp4"

                # Calculate the scrolling speed
                scroll_speed = image.shape[1] / (video_duration * fps)
                # Calculate the total frames
                total_frames = video_duration * fps

                # Create the video
                creator = VideoCreator(image, video_file, total_frames, scroll_speed, video_width, video_height, fps)
                creator.create_video()

                input_file = name + "_raw" + ".mp4"
                output_file = name + ".mp4"

                # Use ffmpeg to convert the video
                command = [ffmpeg_path, '-i', input_file, '-c:v', 'libx264', '-pix_fmt', 'yuv420p', output_file]
                subprocess.run(command, check=True)

                # Remove the raw video file
                os.remove(input_file)

                