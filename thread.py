import soundfile as sf
import numpy as np
from pydub import AudioSegment
import PyPDF2 as p2
import os
import subprocess

def convert_wav_to_mp3(wav_file_path, mp3_file_path, ffmpeg_path):
    # Build the command for subprocess to run
    command = [ffmpeg_path, '-i', wav_file_path, '-ab', '192k', mp3_file_path]
    subprocess.run(command, check=True)

def wind_converter(li, Type, output_filename, wind_file, ffmpeg_path):
    li = li.replace('Selected: ', '')
    wind_data, wind_sample_rate = sf.read(wind_file)
    wind_duration = len(wind_data) / wind_sample_rate

    if Type == 'File':
        sample_rate = 44100  
        wind_intensities = [] 

        file = li.split('\\')
        file = file[len(file)-1]
        name = file.replace('.pdf', '')

        if os.path.exists(name+'.mp3'):
            return
        
        if os.path.exists(name+'.wav'):
            os.remove(name+'.wav')

        pdf_reader = p2.PdfFileReader(li)
        num_pages = pdf_reader.numPages

        for i in range(num_pages):
            page = pdf_reader.getPage(i)
            text = page.extractText()
            text = text.split('\n')
            for i in range(len(text)):
                day_intensities = []  # Create a new list for each day
                row = text[i][2:]
                row = row.split(',')
                for i in range(len(row)):
                    row[i] = row[i].strip()
                print(row)
                for element in row:
                    try:
                        wind_intensity = float(element)
                        day_intensities.append(wind_intensity)  # Add the intensity to the current day's list
                    except ValueError:
                        continue
                wind_intensities.append(day_intensities)  # Add the day's list to the overall list

        output_data = []
        duration_per_day = 30  
        transition_fraction = 1  # Set the transition duration to 10% of the duration for each intensity

        print(wind_intensities)
        print(len(wind_intensities))

        if len(wind_intensities) >= 25:
            #Remove so that it has 24 days
            wind_intensities = wind_intensities[:24]

        print(wind_intensities)
        
        for day in range(len(wind_intensities)):
            print(day)
            duration_per_intensity = duration_per_day / len(wind_intensities[day])
            transition_duration = duration_per_intensity * transition_fraction
            intensity_start = wind_intensities[day][0]
            for i in range(int(sample_rate * duration_per_day)):
                elapsed_time = i / sample_rate
                intensity_index = int(elapsed_time // duration_per_intensity)
                intensity_end = wind_intensities[day][intensity_index]
                if intensity_index == len(wind_intensities[day]) - 1:
                    intensity = intensity_end
                else:
                    t = (elapsed_time - intensity_index * duration_per_intensity) / transition_duration
                    intensity = intensity_start + (intensity_end - intensity_start) * t
                intensity_start = intensity_end
                
                wind_index = int(i % (sample_rate * wind_duration))
                if intensity <= 1:
                    value = wind_data[wind_index] * intensity * 0
                else:
                    value = wind_data[wind_index] * intensity / 15.0

                output_data.append(value)

        output_data = np.array(output_data)
        sf.write(name+'.wav', output_data, sample_rate)
        sound = AudioSegment.from_wav(name+'.wav')
        sound = sound + 3
        sound.export(name+'.wav', format='wav')

        convert_wav_to_mp3(name+'.wav', name+'.mp3', ffmpeg_path)
        os.remove(name+'.wav')

    else:
        for file in os.listdir(li):
            if file.endswith(".pdf"):
                sample_rate = 44100  
                wind_intensities = [] 


                name = file.replace('.pdf', '')
                file = li + '\\' + file
                
                if os.path.exists(name+'.mp3'):
                    return
                
                if os.path.exists(name+'.wav'):
                    os.remove(name+'.wav')

                pdf_reader = p2.PdfFileReader(file)
                num_pages = pdf_reader.numPages

                for i in range(num_pages):
                    page = pdf_reader.getPage(i)
                    text = page.extractText()
                    text = text.split('\n')
                    for i in range(len(text)):
                        day_intensities = []  # Create a new list for each day
                        row = text[i][2:]
                        row = row.split(',')
                        for i in range(len(row)):
                            row[i] = row[i].strip()
                        print(row)
                        for element in row:
                            try:
                                wind_intensity = float(element)
                                day_intensities.append(wind_intensity)  # Add the intensity to the current day's list
                            except ValueError:
                                continue
                        wind_intensities.append(day_intensities)  # Add the day's list to the overall list

                output_data = []
                duration_per_day = 30  
                transition_fraction = 1  # Set the transition duration to 10% of the duration for each intensity

                print(wind_intensities)
                print(len(wind_intensities))

                if len(wind_intensities) >= 25:
                    #Remove so that it has 24 days
                    wind_intensities = wind_intensities[:24]

                print(wind_intensities)
                
                for day in range(len(wind_intensities)):
                    print(day)
                    duration_per_intensity = duration_per_day / len(wind_intensities[day])
                    transition_duration = duration_per_intensity * transition_fraction
                    intensity_start = wind_intensities[day][0]
                    for i in range(int(sample_rate * duration_per_day)):
                        elapsed_time = i / sample_rate
                        intensity_index = int(elapsed_time // duration_per_intensity)
                        intensity_end = wind_intensities[day][intensity_index]
                        if intensity_index == len(wind_intensities[day]) - 1:
                            intensity = intensity_end
                        else:
                            t = (elapsed_time - intensity_index * duration_per_intensity) / transition_duration
                            intensity = intensity_start + (intensity_end - intensity_start) * t
                        intensity_start = intensity_end
                        
                        wind_index = int(i % (sample_rate * wind_duration))
                        value = wind_data[wind_index] * intensity / 15.0
                        output_data.append(value)

                output_data = np.array(output_data)
                sf.write(name+'.wav', output_data, sample_rate)
                sound = AudioSegment.from_wav(name+'.wav')
                sound = sound + 3
                sound.export(name+'.wav', format='wav')

                convert_wav_to_mp3(name+'.wav', name+'.mp3', ffmpeg_path)
                os.remove(name+'.wav')

# Get the directory of the current script
script_dir = os.path.dirname(os.path.abspath(__file__))

# Path to the ffmpeg binary
ffmpeg_path = os.path.join(script_dir, 'ffmpeg', 'ffmpeg.exe')

wind_converter('assets/wind3.pdf', "File", "dfghj", "assets/Wind_Loop.wav", ffmpeg_path)