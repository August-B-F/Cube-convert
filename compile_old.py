import os
import pygame
from pygame.locals import *
from pygame import Rect
import time
import sys
import textwrap
import colorsys
from converters import BPM_MP3, CLOUDS_TO_MP4, RGB_MP4, TEXT_TO_MP4, WIND_TO_MP3
import threading

# Initialize Pygame and font
pygame.init()
pygame.font.init()

# Constants for colors
LIGHT_GREEN = (150, 158, 123)
BLACK = (33, 39, 15)
GREEN = (130, 138, 104)
BLUE = (124,135,172)
DARK_BLUE = (124,135,122)
RED = (191,125,101)
DARK_RED = (141,125,101)
WHITE = (255, 255, 255)
LIGHT_RED = (194,62,62)

#pyinstaller --onefile --icon=favicon.ico --add-data "pixel.ttf;." --add-data "assets/JdLcdRoundedRegular-vXwE.ttf;." --add-data "convertor_logo.png;." --add-data "pixel/folder.png;." --add-data "pixel/file.png;." --add-data "pixel/music.png;." --add-data "pixel/film.png;." --add-data "pixel/img.png;." --add-data "pixel/pdf.png;." --add-data "pixel/exe.png;." compile.py

# Constants for file types and their corresponding icons
folder = os.path.join(sys._MEIPASS, "folder.png")
files = os.path.join(sys._MEIPASS, "file.png")
music = os.path.join(sys._MEIPASS, "music.png")
film = os.path.join(sys._MEIPASS, "film.png")
img = os.path.join(sys._MEIPASS, "img.png")
pdf = os.path.join(sys._MEIPASS, "pdf.png")
exe = os.path.join(sys._MEIPASS, "exe.png")
FILE_ICONS = {
    '.mp3': music,
    '.wav': music,
    '.mp4': film,
    '.png': img,
    '.jpg': img,
    '.jpeg': img,
    '.pdf': pdf,
    '.exe': exe
}

# Constants for layout
COLOR_PICKER_RECT = pygame.Rect(20, 50, 200, 200)
BRIGHTNESS_SLIDER_RECT = pygame.Rect(224, 50, 20, 200)  # Moved 2 pixels to the right
BACKGROUND_COLOR_RECT = pygame.Rect(0, 0, 340, 30)
SELECTED_COLOR_RECT = pygame.Rect(260, 50, 60, 60)
PREMADE_COLORS_RECTS = [pygame.Rect(20 + i*30, 260, 20, 20) for i in range(10)]
WINDOW_RECT = pygame.Rect(200, 200, 340, 300)
TOP_BAR_RECT = pygame.Rect(0, 0, 290, 30)
CLOSE_BUTTON_RECT = pygame.Rect(290, 0, 50, 30)
DRAGGING = False
OFFSET = (0, 0)
COLOR_WINDOW_OPEN = False

RGB_RECTS = [pygame.Rect(260, 120 + i*30, 60, 30) for i in range(3)]

PREMADE_COLORS = [pygame.Color(i*25, i*25, i*25) for i in range(10)]

SCREEN_WIDTH = 800
SCREEN_HEIGHT = 600
PADDING = 10
BUTTON_WIDTH = 40
BUTTON_HEIGHT = 30
BUTTON_PADDING = 20
FILE_CARD_WIDTH = 150
FILE_CARD_HEIGHT = 150
FILE_CARD_PADDING = 20
FILE_CARD_TEXT_HEIGHT = 20
FILE_CARD_ICON_SIZE = 100
FONT_SIZE = 18
SCROLL_SPEED = 30
SCROLL_BAR_WIDTH = 20
SCROLL_BAR_COLOR = BLACK
DOUBLE_CLICK_TIME = 0.5
SEARCH_BAR_MAX_LENGTH = 50

# use pixel.ttf for better looking text
font_file = os.path.join(sys._MEIPASS, "pixel.ttf")

FONT = pygame.font.Font(font_file, FONT_SIZE)
BIG_FONT = pygame.font.Font(font_file, 20)
SMALL_FONT = pygame.font.Font(font_file, 16)

# Tab related constants
TAB_WIDTH = SCREEN_WIDTH // 5
TAB_HEIGHT = 35
TAB_NAMES = ["WIND", "BMP", "CLOUDS", "RGB", "TEXT"]
TAB_DESC = ["Convert wind intensities into an MP3 file. Input can be a list of wind intensities, a folder containing multiple files, or a single file.", 
            "Convert BMP data into an MP3 file. Input can be a list of BMP data, a folder containing multiple files, or a single file.", 
            "Convert cloud images into an MP4 file. Input can be a list of cloud images, a folder containing multiple files, or a single file.", 
            "Convert RGB values into an MP4 file. Input can be an array of RGB values, a folder containing multiple files, or a single file.", 
            "Convert text into an MP4 file. Input can be a list of text, a folder containing multiple files, or a single file."]

# Button related constants
BUTTON_NAMES = ["Folder", "File", "Manual"]
OUTPUTNAME = ""

MODE_SELECTED = False
MANUAL_MODE = False

def draw_color_picker(surface, rect):
    for y in range(0, rect.height, 10):
        for x in range(0, rect.width, 10):
            hue = x / rect.width
            saturation = y / rect.height
            color = colorsys.hsv_to_rgb(hue, saturation, 1)
            pygame.draw.rect(surface, (int(color[0]*255), int(color[1]*255), int(color[2]*255)), (rect.x + x, rect.y + y, 10, 10))

def draw_brightness_slider(surface, rect, brightness):
    for y in range(0, rect.height, 10):
        color = colorsys.hsv_to_rgb(0, 0, 1 - y/rect.height)
        pygame.draw.rect(surface, (int(color[0]*255), int(color[1]*255), int(color[2]*255)), (rect.x, rect.y + y, rect.width, 10))

color_picker_surface = pygame.Surface((WINDOW_RECT.width, WINDOW_RECT.height), pygame.SRCALPHA)
color_picker_surface.fill((0, 0, 0, 0))
draw_color_picker(color_picker_surface, COLOR_PICKER_RECT)

selected_color = pygame.Color(255, 255, 255)
selected_hue_saturation = (0, 0)
selected_brightness = 1


class Tab:
    def __init__(self):
        self.selected = 0
        self.desc = TAB_DESC[self.selected]
        self.name = TAB_NAMES[self.selected]
        self.clicked = False

        self.folder = False
        self.file = False
        self.manual = False
        self.color = False

        self.mode = MODE_SELECTED
        self.color_mode = False
        self.manual_mode = MANUAL_MODE

        self.selected_mode = ""

    def draw(self, surface):
        BUTTON_WIDTH = 400
        BUTTON_HEIGHT = 50
        #draw tabs
        for i in range(len(TAB_NAMES)):
            tab = Rect(i * TAB_WIDTH, 0, TAB_WIDTH, TAB_HEIGHT)
            if self.selected == i:
                pygame.draw.rect(surface, BLACK, tab, border_radius=5)
                tab_text = FONT.render(TAB_NAMES[i], True, LIGHT_GREEN)
                tab_text_rect = tab_text.get_rect(center=tab.center)
                surface.blit(tab_text, tab_text_rect)
            else:
                pygame.draw.rect(surface, LIGHT_GREEN, tab, border_radius=5)
                tab_text = FONT.render(TAB_NAMES[i], True, BLACK)
                tab_text_rect = tab_text.get_rect(center=tab.center)
                surface.blit(tab_text, tab_text_rect)

            if self.mode == False and MANUAL_MODE == False and self.manual_mode == False:

                wrapped_desc = textwrap.wrap(self.desc, width=65)  # Adjust the width as needed
                for i, line in enumerate(wrapped_desc):
                    desc_text = FONT.render(line, True, BLACK)
                    desc_text_rect = desc_text.get_rect(center=(SCREEN_WIDTH // 2, SCREEN_HEIGHT // 4 + i * FONT.get_linesize()))
                    surface.blit(desc_text, desc_text_rect)

                # Folder button and make it centered
                if self.folder == True:
                    folder_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2, BUTTON_WIDTH, BUTTON_HEIGHT)
                    pygame.draw.rect(surface, BLACK, folder_button, border_radius=5)
                    pygame.draw.rect(surface, BLACK, folder_button, 3, border_radius=5)  # Draw border
                    folder_text = BIG_FONT.render('Folder', True, LIGHT_GREEN)
                    folder_text_rect = folder_text.get_rect(center=folder_button.center)
                    surface.blit(folder_text, folder_text_rect)                    

                else: 
                    folder_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2, BUTTON_WIDTH, BUTTON_HEIGHT)
                    pygame.draw.rect(surface, LIGHT_GREEN, folder_button, border_radius=5)
                    pygame.draw.rect(surface, BLACK, folder_button, 3, border_radius=5)  # Draw border
                    folder_text = BIG_FONT.render('Folder', True, BLACK)
                    folder_text_rect = folder_text.get_rect(center=folder_button.center)
                    surface.blit(folder_text, folder_text_rect)


                # File button move down by padding + height
                if self.file == True:
                    file_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + BUTTON_HEIGHT + BUTTON_PADDING, BUTTON_WIDTH, BUTTON_HEIGHT)
                    pygame.draw.rect(surface, BLACK, file_button, border_radius=5)
                    pygame.draw.rect(surface, BLACK, file_button, 3, border_radius=5)  # Draw border
                    file_text = BIG_FONT.render('File', True, LIGHT_GREEN)
                    file_text_rect = file_text.get_rect(center=file_button.center)
                    surface.blit(file_text, file_text_rect)                    

                else:
                    file_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + BUTTON_HEIGHT + BUTTON_PADDING, BUTTON_WIDTH, BUTTON_HEIGHT)
                    pygame.draw.rect(surface, LIGHT_GREEN, file_button, border_radius=5)
                    pygame.draw.rect(surface, BLACK, file_button, 3, border_radius=5)  # Draw border
                    file_text = BIG_FONT.render('File', True, BLACK)
                    file_text_rect = file_text.get_rect(center=file_button.center)
                    surface.blit(file_text, file_text_rect)

                # Manual button
                if self.name != "CLOUDS": 
                    if self.manual == True:
                        manual_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + 2 * (BUTTON_HEIGHT + BUTTON_PADDING), BUTTON_WIDTH, BUTTON_HEIGHT)
                        pygame.draw.rect(surface, BLACK, manual_button, border_radius=5)
                        pygame.draw.rect(surface, BLACK, manual_button, 3, border_radius=5)
                        manual_text = BIG_FONT.render('Manual', True, LIGHT_GREEN)
                        manual_text_rect = manual_text.get_rect(center=manual_button.center)
                        surface.blit(manual_text, manual_text_rect)                    
                    else:
                        manual_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + 2 * (BUTTON_HEIGHT + BUTTON_PADDING), BUTTON_WIDTH, BUTTON_HEIGHT)
                        pygame.draw.rect(surface, LIGHT_GREEN, manual_button, border_radius=5)
                        pygame.draw.rect(surface, BLACK, manual_button, 3, border_radius=5)
                        manual_text = BIG_FONT.render('Manual', True, BLACK)
                        manual_text_rect = manual_text.get_rect(center=manual_button.center)
                        surface.blit(manual_text, manual_text_rect)
                
                # Color picker button
                if self.name == "TEXT":
                    if self.color == True:
                        color_picker_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + 3 * (BUTTON_HEIGHT + BUTTON_PADDING), BUTTON_WIDTH, BUTTON_HEIGHT)
                        pygame.draw.rect(surface, BLACK, color_picker_button, border_radius=5)
                        pygame.draw.rect(surface, BLACK, color_picker_button, 3, border_radius=5)
                        color_picker_text = BIG_FONT.render('Color Picker', True, LIGHT_GREEN)
                        color_picker_text_rect = color_picker_text.get_rect(center=color_picker_button.center)
                        surface.blit(color_picker_text, color_picker_text_rect)
                    else:
                        color_picker_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + 3 * (BUTTON_HEIGHT + BUTTON_PADDING), BUTTON_WIDTH, BUTTON_HEIGHT)
                        pygame.draw.rect(surface, LIGHT_GREEN, color_picker_button, border_radius=5)
                        pygame.draw.rect(surface, BLACK, color_picker_button, 3, border_radius=5)
                        color_picker_text = BIG_FONT.render('Color Picker', True, BLACK)
                        color_picker_text_rect = color_picker_text.get_rect(center=color_picker_button.center)
                        surface.blit(color_picker_text, color_picker_text_rect)

    def update(self):
        BUTTON_WIDTH = 400
        BUTTON_HEIGHT = 50

        self.folder = False
        self.file = False
        self.manual = False
        self.color = False

        # Check if i clicked anything in the tab or another tab
        if pygame.mouse.get_pressed()[0]:
            mouse_pos = pygame.mouse.get_pos()
            for i in range(len(TAB_NAMES)):
                tab = Rect(i * TAB_WIDTH, 0, TAB_WIDTH, TAB_HEIGHT)
                if tab.collidepoint(mouse_pos):
                    self.selected = i
                    self.name = TAB_NAMES[i]
                    self.desc = TAB_DESC[i]
                    self.mode = False
                    self.manual_mode = False
                    break

            self.clicked = True
            folder_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2, BUTTON_WIDTH, BUTTON_HEIGHT)
            file_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + BUTTON_HEIGHT + BUTTON_PADDING, BUTTON_WIDTH, BUTTON_HEIGHT)
            manual_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + 2 * (BUTTON_HEIGHT + BUTTON_PADDING), BUTTON_WIDTH, BUTTON_HEIGHT)
            color_picker_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + 3 * (BUTTON_HEIGHT + BUTTON_PADDING), BUTTON_WIDTH, BUTTON_HEIGHT)

            if folder_button.collidepoint(mouse_pos):
                self.folder = True
            elif file_button.collidepoint(mouse_pos):
                self.file = True
            elif manual_button.collidepoint(mouse_pos):
                self.manual = True
            elif color_picker_button.collidepoint(mouse_pos):
                self.color = True


        elif pygame.mouse.get_pressed()[0] == False and self.clicked == True:
            mouse_pos = pygame.mouse.get_pos()
            pygame.time.wait(100)
            if self.mode == False and self.manual_mode == False:

                folder_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2, BUTTON_WIDTH, BUTTON_HEIGHT)
                file_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + BUTTON_HEIGHT + BUTTON_PADDING, BUTTON_WIDTH, BUTTON_HEIGHT)
                manual_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + 2 * (BUTTON_HEIGHT + BUTTON_PADDING), BUTTON_WIDTH, BUTTON_HEIGHT)
                color_picker_button = Rect((SCREEN_WIDTH - BUTTON_WIDTH) // 2, SCREEN_HEIGHT // 2 + 3 * (BUTTON_HEIGHT + BUTTON_PADDING), BUTTON_WIDTH, BUTTON_HEIGHT)

                if folder_button.collidepoint(mouse_pos):
                    self.mode = True
                    self.selected_mode = "Folder"
                
                if file_button.collidepoint(mouse_pos):
                    self.mode = True
                    self.selected_mode = "File"
                
                if manual_button.collidepoint(mouse_pos):
                    self.manual_mode = True
                    self.selected_mode = "Manually"
                
                if color_picker_button.collidepoint(mouse_pos):
                    self.color_mode = True

            self.clicked = False
            self.foler = False
            self.file = False
            self.manual = False
            self.color = False

class Converter:
    def __init__(self):
        self.is_loading = False

    def convert(self, selected, Type, outputname, name):
        try:
            # turn this {'C:\\Mina_project\\Job\\QR\\software\\assets\\16001.pdf'} into this 'C:\\Mina_project\\Job\\QR\\software\\assets\\16001.pdf'
            # name = name[0] wont work 
            name = list(name)[0]
            # make output filename field white
            if selected == 'WIND_TO_MP3':
                WIND_TO_MP3.wind_converter(name, Type, outputname)
            elif selected == 'BPM_MP3':
                BPM_MP3.BPM_converter(name, Type, outputname)
            elif selected == 'CLOUDS_TO_MP4':
                CLOUDS_TO_MP4.CLOUDS_converter(name, Type)
            elif selected == 'RGB_MP4':
                RGB_MP4.RGB_converter(name, Type, outputname)
            elif selected == 'TEXT_TO_MP4':
                font_path = os.path.join(sys._MEIPASS, "JdLcdRoundedRegular-vXwE.ttf")
                color = '#{:02x}{:02x}{:02x}'.format(selected_color.r, selected_color.g, selected_color.b)
                TEXT_TO_MP4.MP4_converter(name, Type, color, outputname, font_path)

            file_selector.is_successful = True
            self.is_loading = False

        except Exception as e:
            print(e)
            file_selector.is_successful = False
            self.is_loading = False

    def loading_bar(self, screen, bar_color, border_color, speed, max_bar_width, min_bar_width, bar_height, border_radius, bar_margin, file_selector, tab):
        start_ticks = pygame.time.get_ticks() #starter tick
        self.is_loading = True
        direction = speed
        position = 0
        max_position = 200 - min_bar_width - 2 * bar_margin
        last_width = max_bar_width
        screen_width, screen_height = screen.get_size()
        bar_x = (screen_width - 200) // 2
        bar_y = (screen_height - 50) // 2

        # Make thread to convert files
        selected = tab.name
        if selected == 'WIND':
            thread = threading.Thread(target=self.convert, args=("WIND_TO_MP3", tab.selected_mode, OUTPUTNAME, file_selector.selected_files)) 
        elif selected == 'BMP':
            thread = threading.Thread(target=self.convert, args=("BPM_MP3", tab.selected_mode, OUTPUTNAME, file_selector.selected_files))
        elif selected == 'CLOUDS':
            thread = threading.Thread(target=self.convert, args=("CLOUDS_TO_MP4", tab.selected_mode, OUTPUTNAME, file_selector.selected_files))
        elif selected == 'RGB':
            thread = threading.Thread(target=self.convert, args=("RGB_MP4", tab.selected_mode, OUTPUTNAME, file_selector.selected_files))
        elif selected == 'TEXT':
            thread = threading.Thread(target=self.convert, args=("TEXT_TO_MP4", tab.selected_mode, OUTPUTNAME, file_selector.selected_files))
        
        thread.start()

        while self.is_loading:
            file_selector.draw(screen)

            if file_selector.selected_files == set() or file_selector.selected_files == None:
                pygame.draw.rect(screen, BLACK, file_selector.submit_button, border_radius=5)
                pygame.draw.rect(screen, BLACK, file_selector.submit_button, 3, border_radius=5)
                submit_text = BIG_FONT.render('Submit', True, LIGHT_GREEN)
            else:
                pygame.draw.rect(screen, LIGHT_GREEN, file_selector.submit_button, border_radius=5)
                pygame.draw.rect(screen, BLACK, file_selector.submit_button, 3, border_radius=5)  # Draw border
                submit_text = BIG_FONT.render('Submit', True, BLACK)
            
            submit_text_rect = submit_text.get_rect(center=file_selector.submit_button.center)

            screen.blit(submit_text, submit_text_rect)

            for event in pygame.event.get():
                if event.type == pygame.QUIT:
                    sys.exit()
            
            tab.draw(screen)

            s = pygame.Surface((screen_width,screen_height))  # the size of your rect
            s.set_alpha(128)                # alpha level
            s.fill((0,0,0))           # this fills the entire surface
            screen.blit(s, (0,0))    # (0,0) are the top-left coordinates

            pygame.draw.rect(screen, border_color, pygame.Rect(bar_x, bar_y, 200, 50), 2, border_radius=border_radius) # draw loading bar border
            position += direction
            if position > max_position:
                direction = -speed
            elif position < 0:
                direction = speed
            width = max_bar_width - abs((position - max_position / 2) / (max_position / 2)) * (max_bar_width - min_bar_width) # width decreases as the bar moves away from the center
            position += (last_width - width) / 2 # compensate for the change in width
            last_width = width
            pygame.draw.rect(screen, bar_color, pygame.Rect(bar_x+position+bar_margin, bar_y+10, width, bar_height), border_radius=border_radius) # draw loading bar

            pygame.display.flip()
            pygame.time.wait(60)
        
        thread.join()
        file_selector.update_file_cards()

class FileCard:
    def __init__(self, name, path, is_directory, rect, success):
        self.name = name
        self.path = path
        self.is_directory = is_directory
        self.rect = rect
        self.selected = False
        self.success = success

    def draw(self, surface, scroll):
        rect = self.rect.move(0, -scroll)
        pygame.draw.rect(surface, LIGHT_GREEN if not self.selected else GREEN, rect, border_radius=10)

        if self.selected:
            if self.success == None:
                pygame.draw.rect(surface, GREEN, rect, border_radius=10)
                pygame.draw.rect(surface, BLACK, rect, 3, border_radius=5)  # Draw border
            elif self.success == True:
                pygame.draw.rect(surface, DARK_BLUE, rect, border_radius=10)
                pygame.draw.rect(surface, BLUE, rect, 3, border_radius=5)
            else:
                pygame.draw.rect(surface, DARK_RED, rect, border_radius=10)            
                pygame.draw.rect(surface, RED, rect, 3, border_radius=5)
        else:
            pygame.draw.rect(surface, LIGHT_GREEN, rect, border_radius=10)
            
            
        # if file is a directory, draw a folder icon
        if self.is_directory:
            icon = FILE_ICONS.get(os.path.splitext(self.name)[1], folder)
        else: 
            icon = FILE_ICONS.get(os.path.splitext(self.name)[1], files)

        surface.blit(pygame.transform.scale(pygame.image.load(icon), (FILE_CARD_ICON_SIZE, FILE_CARD_ICON_SIZE)), rect.move((self.rect.width - FILE_CARD_ICON_SIZE) // 2, PADDING))

        # Truncate long file names
        display_name = self.name
        while FONT.size(display_name + '...')[0] > self.rect.width - 10:
            display_name = display_name[:-1]
        display_name += '...' if display_name != self.name else ''

        name_text = FONT.render(display_name, True, BLACK)
        name_text_rect = name_text.get_rect(midtop=(self.rect.centerx, rect.bottom - FILE_CARD_TEXT_HEIGHT))
        surface.blit(name_text, name_text_rect)


class FileSelector:
    def __init__(self, screen):
        self.current_path = os.getcwd()
        self.cards = []
        self.scroll = 0
        self.back_button = Rect(70, 50, BUTTON_WIDTH, BUTTON_HEIGHT)
        self.search_box = Rect(120, 50, 610, BUTTON_HEIGHT)
        self.search_text = ''
        self.search_pretext = 'Search...'
        self.search_box_active = False
        self.scroll_bar = Rect(SCREEN_WIDTH - SCROLL_BAR_WIDTH, 0, SCROLL_BAR_WIDTH, 0)
        self.scroll_drag = False
        self.last_click_time = 0
        self.selected_files = set()
        self.is_successful = None
        self.update_file_cards()
        self.cursor_visible = True
        self.cursor_counter = 0
        self.submit_button = Rect(SCREEN_WIDTH - 170, SCREEN_HEIGHT - 50, 100, BUTTON_HEIGHT)
        self.submit_button_text = 'Submit'
        self.screen = screen
        self.back_button_clicked = False

    def update_file_cards(self):
        self.cards.clear()
        entries = [entry for entry in os.listdir(self.current_path) if entry.lower().startswith(self.search_text.lower())]
        entries.sort(key=lambda x: (not os.path.isdir(os.path.join(self.current_path, x)), os.path.splitext(x)[1] != '.pdf', x)) # Sort by folders, then pdfs, then files
        row = 0
        col = 0
        for entry in entries:
            path = os.path.join(self.current_path, entry)
            if os.path.isdir(path) or os.path.isfile(path):
                rect = Rect((SCREEN_WIDTH - 4 * FILE_CARD_WIDTH - 3 * FILE_CARD_PADDING) // 2 + col * (FILE_CARD_WIDTH + FILE_CARD_PADDING),
                            self.search_box.bottom + PADDING + row * (FILE_CARD_HEIGHT + FILE_CARD_PADDING + 10),
                            FILE_CARD_WIDTH,
                            FILE_CARD_HEIGHT)
                file_card = FileCard(entry, path, os.path.isdir(path), rect, self.is_successful)
                if path in self.selected_files:
                    file_card.selected = True
                self.cards.append(file_card)
                col += 1
                if col == 4:
                    col = 0
                    row += 1

        # Update total_height
        self.total_height = max(SCREEN_HEIGHT, self.search_box.bottom + PADDING + row * (FILE_CARD_HEIGHT + FILE_CARD_PADDING) + SCREEN_HEIGHT // 2)

        # Update scroll bar
        self.scroll_bar.height = min(SCREEN_HEIGHT * SCREEN_HEIGHT // self.total_height, SCREEN_HEIGHT)
        self.scroll_bar.top = min(max(self.scroll * SCREEN_HEIGHT // self.total_height, 0), SCREEN_HEIGHT - self.scroll_bar.height)

    def handle_event(self, event):
        if event.type == KEYDOWN and self.search_box_active:
            if event.key == K_BACKSPACE:
                self.search_text = self.search_text[:-1]
            elif len(self.search_text) < SEARCH_BAR_MAX_LENGTH and event.unicode.isprintable():
                self.search_text += event.unicode
            self.scroll = 0
            self.update_file_cards()
        elif event.type == MOUSEBUTTONDOWN:
            if self.scroll_bar.collidepoint(event.pos):
                self.scroll_drag = True
            elif self.search_box.collidepoint(event.pos):
                self.search_box_active = True
                if self.search_text == self.search_pretext:
                    self.search_text = ''
            elif self.back_button.collidepoint(event.pos) and event.button == 1:
                self.back_button_clicked = True
            elif self.submit_button.collidepoint(event.pos) and event.button == 1:
                pass
            else:
                self.search_box_active = False
                for card in self.cards:
                    if card.rect.move(0, -self.scroll).collidepoint(event.pos):
                        if card.is_directory and event.button == 1 and time.time() - self.last_click_time < DOUBLE_CLICK_TIME and card.path in self.selected_files:
                            self.is_successful = None
                            self.selected_files.clear()
                            self.current_path = card.path
                            self.scroll = 0
                            self.update_file_cards()
                        elif event.button == 1:
                            if card.selected and self.is_successful != None:
                                self.is_successful = None
                                self.update_file_cards()
                            elif card.selected and self.is_successful == None:
                                self.selected_files.remove(card.path)
                            else:
                                self.is_successful = None
                                self.selected_files.clear()
                                self.selected_files.add(card.path)
                                self.update_file_cards()
                            card.selected = not card.selected
                        break
                self.last_click_time = time.time()
        elif event.type == MOUSEBUTTONUP:
            if self.scroll_drag:
                self.scroll_drag = False
            if self.back_button_clicked and self.back_button.collidepoint(event.pos) and event.button == 1:
                pygame.time.wait(10)
                self.current_path = os.path.dirname(self.current_path)
                self.scroll = 0
                self.update_file_cards()
                self.back_button_clicked = False
        elif event.type == MOUSEMOTION:
            if self.scroll_drag:
                self.scroll = min(max(self.scroll + event.rel[1] * self.total_height / SCREEN_HEIGHT, 0), self.total_height - SCREEN_HEIGHT)
                self.scroll_bar.top = min(max(self.scroll * SCREEN_HEIGHT // self.total_height, 0), SCREEN_HEIGHT - self.scroll_bar.height)
        if event.type == MOUSEWHEEL:
            if not self.scroll_drag:
                self.scroll = min(max(self.scroll - event.y * SCROLL_SPEED, 0), self.total_height - SCREEN_HEIGHT) 
                self.scroll_bar.top = min(max(self.scroll * SCREEN_HEIGHT / self.total_height, 0), SCREEN_HEIGHT - self.scroll_bar.height)

    def draw(self, surface):
        surface.fill(LIGHT_GREEN)
        for card in self.cards:
            card.draw(surface, self.scroll)
        pygame.draw.rect(surface, GREEN if self.search_box_active else LIGHT_GREEN, self.search_box, border_radius=5)
        pygame.draw.rect(surface, BLACK, self.search_box, 3, border_radius=5)  # Draw border
        if self.search_box_active:
            search_text = FONT.render(self.search_text, True, BLACK)
            surface.blit(search_text, (self.search_box.left + PADDING, self.search_box.centery - search_text.get_height() // 2))
            self.cursor_counter += 1
            if self.cursor_counter % 40 < 20:  # Change this to make the cursor blink faster
                pygame.draw.line(surface, BLACK, (self.search_box.left + FONT.size(self.search_text)[0] + PADDING, self.search_box.bottom - 7), (self.search_box.left + FONT.size(self.search_text)[0] + 20, self.search_box.bottom - 7), 2)
        else:
            search_pretext = FONT.render(self.search_pretext, True, BLACK)
            surface.blit(search_pretext, (self.search_box.left + PADDING, self.search_box.centery - search_pretext.get_height() // 2))
        
        if self.back_button_clicked:
            pygame.draw.rect(surface, BLACK, self.back_button, border_radius=5)
            pygame.draw.rect(surface, BLACK, self.back_button, 3, border_radius=5)  # Draw border
            back_text = BIG_FONT.render('<', True, LIGHT_GREEN)
            back_text_rect = back_text.get_rect(center=self.back_button.center)
        else:
            pygame.draw.rect(surface, LIGHT_GREEN, self.back_button, border_radius=5)
            pygame.draw.rect(surface, BLACK, self.back_button, 3, border_radius=5)  # Draw border
            back_text = BIG_FONT.render('<', True, BLACK)
            back_text_rect = back_text.get_rect(center=self.back_button.center)

        surface.blit(back_text, back_text_rect)
        pygame.draw.rect(surface, BLACK if self.scroll_drag else GREEN, self.scroll_bar, border_radius=5)


pygame.display.set_caption('CONVERTER')
# change the icon
image_file = os.path.join(sys._MEIPASS, "convertor_logo.png")
icon = pygame.image.load(image_file)
pygame.display.set_icon(icon)
screen = pygame.display.set_mode((SCREEN_WIDTH, SCREEN_HEIGHT), pygame.RESIZABLE + pygame.SCALED)
clock = pygame.time.Clock()
file_selector = FileSelector(screen)

#tabs 
tab = Tab()
converter = Converter()

clicked = False
running = True

while running:

    screen.fill(LIGHT_GREEN)
    COLOR_WINDOW_OPEN = tab.color_mode

    for event in pygame.event.get():
        if event.type == QUIT:
            running = False
            sys.exit()
        else:
            if tab.mode and not COLOR_WINDOW_OPEN:
                file_selector.handle_event(event)

        if event.type == MOUSEBUTTONDOWN and event.button == 1:
            if file_selector.submit_button.collidepoint(event.pos) == True and event.button == 1 and file_selector.selected_files != set() and file_selector.selected_files != None and not COLOR_WINDOW_OPEN and tab.mode:
                clicked = True

            if COLOR_WINDOW_OPEN and TOP_BAR_RECT.move(WINDOW_RECT.topleft).collidepoint(event.pos):
                DRAGGING = True
                OFFSET = (WINDOW_RECT.x - event.pos[0], WINDOW_RECT.y - event.pos[1])
            elif COLOR_WINDOW_OPEN and CLOSE_BUTTON_RECT.move(WINDOW_RECT.topleft).collidepoint(event.pos):
                tab.color_mode = False
            elif COLOR_WINDOW_OPEN and COLOR_PICKER_RECT.move(WINDOW_RECT.topleft).collidepoint(event.pos):
                selected_hue_saturation = ((event.pos[0] - COLOR_PICKER_RECT.x - WINDOW_RECT.x) / COLOR_PICKER_RECT.width,
                                        (event.pos[1] - COLOR_PICKER_RECT.y - WINDOW_RECT.y) / COLOR_PICKER_RECT.height)
                selected_color.hsva = (selected_hue_saturation[0]*360, selected_hue_saturation[1]*100, selected_brightness*100)
            elif COLOR_WINDOW_OPEN and BRIGHTNESS_SLIDER_RECT.move(WINDOW_RECT.topleft).collidepoint(event.pos):
                selected_brightness = 1 - (event.pos[1] - BRIGHTNESS_SLIDER_RECT.y - WINDOW_RECT.y) / BRIGHTNESS_SLIDER_RECT.height
                selected_color.hsva = (selected_hue_saturation[0]*360, selected_hue_saturation[1]*100, selected_brightness*100)
            elif COLOR_WINDOW_OPEN:
                for i, rect in enumerate(PREMADE_COLORS_RECTS):
                    if rect.move(WINDOW_RECT.topleft).collidepoint(event.pos):
                        selected_color = PREMADE_COLORS[i]

        elif event.type == MOUSEBUTTONUP and event.button == 1 and tab.mode and not COLOR_WINDOW_OPEN:
            if file_selector.submit_button.collidepoint(event.pos) == True and event.button == 1 and file_selector.selected_files != set() and file_selector.selected_files != None:
                pygame.time.wait(100)
                converter.loading_bar(file_selector.screen, LIGHT_GREEN, LIGHT_GREEN, 5, 50, 10, 30, 5, 10, file_selector, tab)
                clicked = False
        elif event.type == MOUSEBUTTONUP: 
            DRAGGING = False
        elif event.type == pygame.MOUSEMOTION:
            if DRAGGING:
                WINDOW_RECT.x = event.pos[0] + OFFSET[0]
                WINDOW_RECT.y = event.pos[1] + OFFSET[1]

    if tab.mode: 
        file_selector.draw(screen)

        if file_selector.selected_files == set() or file_selector.selected_files == None:
            pygame.draw.rect(screen, BLACK, file_selector.submit_button, border_radius=5)
            pygame.draw.rect(screen, BLACK, file_selector.submit_button, 3, border_radius=5)
            submit_text = BIG_FONT.render('Submit', True, LIGHT_GREEN)
        else:
            if clicked:
                pygame.draw.rect(screen, BLACK, file_selector.submit_button, border_radius=5)
                pygame.draw.rect(screen, BLACK, file_selector.submit_button, 3, border_radius=5)
                submit_text = BIG_FONT.render('Submit', True, LIGHT_GREEN)

            else:
                pygame.draw.rect(screen, LIGHT_GREEN, file_selector.submit_button, border_radius=5)
                pygame.draw.rect(screen, BLACK, file_selector.submit_button, 3, border_radius=5)  # Draw border
                submit_text = BIG_FONT.render('Submit', True, BLACK)
        
        submit_text_rect = submit_text.get_rect(center=file_selector.submit_button.center)

        screen.blit(submit_text, submit_text_rect)

    if tab.manual_mode: 
        # Draw text "input data here:"
        input_text = BIG_FONT.render('Input data here:', True, BLACK)
        input_text_rect = input_text.get_rect(center=(SCREEN_WIDTH/2, SCREEN_HEIGHT/2 - 100))
        screen.blit(input_text, input_text_rect)

    
    if not COLOR_WINDOW_OPEN:
        tab.update()
    tab.draw(screen)

    if COLOR_WINDOW_OPEN:
        s = pygame.Surface((SCREEN_WIDTH, SCREEN_HEIGHT))  # the size of your rect
        s.set_alpha(128)                # alpha level
        s.fill((0,0,0))           # this fills the entire surface
        screen.blit(s, (0,0))    # (0,0) are the top-left coordinates
    
        pygame.draw.rect(screen, LIGHT_GREEN, WINDOW_RECT, border_radius=5)
        pygame.draw.rect(screen, BLACK, WINDOW_RECT, 3, border_radius=5)
        pygame.draw.rect(screen, GREEN, TOP_BAR_RECT.move(WINDOW_RECT.topleft), border_radius=5)
        pygame.draw.rect(screen, BLACK, BACKGROUND_COLOR_RECT.move(WINDOW_RECT.topleft), 3, border_radius=5)
        pygame.draw.rect(screen, LIGHT_RED, CLOSE_BUTTON_RECT.move(WINDOW_RECT.topleft), border_radius=5)
        screen.blit(FONT.render('x', True, WHITE), CLOSE_BUTTON_RECT.move(WINDOW_RECT.topleft).move(19, 5))  # Centered 'X'
        pygame.draw.rect(screen, BLACK, CLOSE_BUTTON_RECT.move(WINDOW_RECT.topleft), 3, border_radius=5)

        screen.blit(SMALL_FONT.render('Color Picker', True, BLACK), TOP_BAR_RECT.move(WINDOW_RECT.topleft).move(7, 7))  # Added 'Color Picker' text
        screen.blit(color_picker_surface, WINDOW_RECT)
        draw_brightness_slider(screen, BRIGHTNESS_SLIDER_RECT.move(WINDOW_RECT.topleft), selected_brightness)
        pygame.draw.rect(screen, selected_color, SELECTED_COLOR_RECT.move(WINDOW_RECT.topleft), border_radius=5)
        pygame.draw.rect(screen, BLACK, SELECTED_COLOR_RECT.move(WINDOW_RECT.topleft), 3, border_radius=5)
        for i, rect in enumerate(PREMADE_COLORS_RECTS):
            pygame.draw.rect(screen, PREMADE_COLORS[i], rect.move(WINDOW_RECT.topleft), border_radius=5)
            #boarder
            pygame.draw.rect(screen, BLACK, rect.move(WINDOW_RECT.topleft), 3, border_radius=5)
        
        # color picker border
        pygame.draw.rect(screen, BLACK, COLOR_PICKER_RECT.move(WINDOW_RECT.topleft), 3, border_radius=1)
        # brightness slider border
        pygame.draw.rect(screen, BLACK, BRIGHTNESS_SLIDER_RECT.move(WINDOW_RECT.topleft), 3, border_radius=1)

        # Draw rgb values
        screen.blit(SMALL_FONT.render('R:' + str(selected_color.r), True, BLACK), RGB_RECTS[0].move(WINDOW_RECT.topleft).move(1, 7))
        screen.blit(SMALL_FONT.render('G:' + str(selected_color.g), True, BLACK), RGB_RECTS[1].move(WINDOW_RECT.topleft).move(1, 7))
        screen.blit(SMALL_FONT.render('B:' + str(selected_color.b), True, BLACK), RGB_RECTS[2].move(WINDOW_RECT.topleft).move(1, 7))

    pygame.display.flip()
    clock.tick(60)

pygame.quit()
