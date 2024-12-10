import os
import struct
import wave
from pydub import AudioSegment
import PyPDF2 as p2
import subprocess

def convert_wav_to_mp3(wav_file_path, mp3_file_path, ffmpeg_path):
    # Build the command for subprocess to run
    command = [
        ffmpeg_path, '-i', wav_file_path, "-vn", "-ar", "48000", "-ac", "2", "-b:a", "320k", mp3_file_path
    ]
    subprocess.run(command, check=True)

def BPM_converter(li, Type, output_filename, ffmpeg_path):
    li = li.replace('Selected: ', '')

    total_duration = 12 * 60
    framerate = 3000.0
    amplitude = 32000   

    if Type == 'File':
        file = li.split('\\')
        file = file[len(file)-1]
        name = file.replace('.pdf', '')

        if os.path.exists(name+'.mp3'):
            return "copy"
        
        #if wav file exists, delete it
        if os.path.exists(name+'.wav'):
            os.remove(name+'.wav')

        mywav = wave.open(name+'.wav', 'w')
        mywav.setparams((2, 2, framerate, 0, 'NONE', 'NONE'))

        pdf_reader = p2.PdfFileReader(li)
        num_pages = pdf_reader.numPages
        BTM_list = []

        for i in range(num_pages):
            page = pdf_reader.getPage(i)
            text = page.extractText()
            text = text.split('\n')

            print(text)

            for i in range(len(text)):
                text[i] = text[i][1:]
                text[i] = text[i][1:]

            text = ''.join(text)

            #remove spaces 
            text = text.replace(' ', '')

            print(text)

            for n in range(int(len(text)/3)):
                BTM_list.append(int(text[n*3:n*3+3]))

        pulse_duration = (total_duration / len(BTM_list))*1.0255

        for x in range(len(BTM_list)):
            try: 
                for i in range(int((BTM_list[x]/60)*pulse_duration)):
                    
                    data = struct.pack('!I', 0)
                    mywav.writeframesraw(data)
                    
                    for n in range(int((60/BTM_list[x] * 2) * framerate)):
                        mywav.writeframes(struct.pack('h', int(amplitude)))
            except:
                continue

        mywav.close()

        audio = AudioSegment.from_file(name+".wav", format="wav")

        length = len(audio)

        twelve_minutes = 12 * 60 * 1000 

        if len(audio) < twelve_minutes:
            last_bpm = BTM_list[-1]
            remaining_time = twelve_minutes - len(audio)
            remaining_beats = int((remaining_time / 1000) * (last_bpm / 60))

            additional_beats = b""
            for i in range(remaining_beats):
                for n in range(int((60/last_bpm * 2) * framerate)):
                    additional_beats += struct.pack('h', int(amplitude))

            additional_audio = AudioSegment(additional_beats, frame_rate=framerate, channels=2, sample_width=2)
            audio += additional_audio

        audio = audio[:twelve_minutes]

        audio = audio + 10

        audio.export(name+".wav", format="wav")

        convert_wav_to_mp3(name+'.wav', name+'.mp3', ffmpeg_path)
        os.remove(name+'.wav')

    else: 
        for file in os.listdir(li):
            if file.endswith(".pdf"):
                name = file.replace('.pdf', '')
                file = li + '\\' + file

                print(file)

                if os.path.exists(name+'.mp3'):
                    continue
            
                if os.path.exists(name+'.wav'):
                    os.remove(name+'.wav')


                mywav = wave.open(name+'.wav', 'w')
                mywav.setparams((2, 2, framerate, 0, 'NONE', 'NONE'))

                pdf_reader = p2.PdfFileReader(file)
                num_pages = pdf_reader.numPages
                BTM_list = []

                for i in range(num_pages):
                    page = pdf_reader.getPage(i)
                    text = page.extractText()
                    text = text.split('\n')

                    print(text)

                    for i in range(len(text)):
                        text[i] = text[i][1:]
                        text[i] = text[i][1:]

                    text = ''.join(text)

                    #remove spaces 
                    text = text.replace(' ', '')

                    print(text)

                    for n in range(int(len(text)/3)):
                        BTM_list.append(int(text[n*3:n*3+3]))

                pulse_duration = (total_duration / len(BTM_list))*1.0255

                for x in range(len(BTM_list)):
                    try: 
                        for i in range(int((BTM_list[x]/60)*pulse_duration)):
                            
                            data = struct.pack('!I', 0)
                            mywav.writeframesraw(data)
                            
                            for n in range(int((60/BTM_list[x] * 2) * framerate)):
                                mywav.writeframes(struct.pack('h', int(amplitude)))
                    except:
                        continue

                mywav.close()

                audio = AudioSegment.from_file(name+".wav", format="wav")
                audio = audio + 10

                length = len(audio)

                twelve_minutes = 12 * 60 * 1000 

                audio = audio[:twelve_minutes]

                audio.export(name+".wav", format="wav")

                convert_wav_to_mp3(name+'.wav', name+'.mp3', ffmpeg_path)
                os.remove(name+'.wav')
