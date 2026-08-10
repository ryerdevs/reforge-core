# draft from https://metin2.dev/topic/30077-official-loot-filter-reversed/

import ui
import uiCommon
import uiScriptLocale
import app
import uiToolTip
import wndMgr

import os
import json
import player
import item
import chr

from _weakref import proxy

LOOTING_SYSTEM_IMG_PATH = "d:/ymir work/ui/game/looting/"

REFINE_MIN = 0
REFINE_MAX = 200

WEARING_LEVEL_MIN = 0
WEARING_LEVEL_MAX = 120

DEFAULT_ONOFF = True

SCROLL_STEP = 0.066

class Range:
	RANGE_HEIGHT = 16

	def __init__(self, root_parent, parent, title, min, max, key):
		self.x = 0
		self.y = 0
		self.height = self.RANGE_HEIGHT

		self.min = min
		self.max = max
		self.key = key

		self.desc_title = ui.ImageBox()
		self.desc_title.SetParent(parent)
		self.desc_title.AddFlag("not_pick")
		self.desc_title.LoadImage(LOOTING_SYSTEM_IMG_PATH + "sub_topic_title_bg.sub")
		self.desc_title.Show()

		self.desc_title_text = ui.TextLine()
		self.desc_title_text.SetParent(self.desc_title)
		self.desc_title_text.SetPosition(self.desc_title.GetWidth() / 2, self.desc_title.GetHeight() / 2)
		self.desc_title_text.SetVerticalAlignCenter()
		self.desc_title_text.SetHorizontalAlignCenter()
		self.desc_title_text.SetText(title)
		self.desc_title_text.Show()

		self.range_img = ui.ImageBox()
		self.range_img.SetParent(parent)
		self.range_img.AddFlag("not_pick")
		self.range_img.LoadImage(LOOTING_SYSTEM_IMG_PATH + "range_value_bg.sub")
		self.range_img.Show()

		self.range_min_value = ui.EditLine()
		self.range_min_value.SetParent(self.range_img)
		self.range_min_value.SetPosition(2, 3)
		self.range_min_value.SetSize(36, 15)
		self.range_min_value.SetNumberMode()
		self.range_min_value.SetMax(3)
		self.range_min_value.SetText(str(self.min))
		self.range_min_value.SetEscapeEvent(ui.__mem_func__(root_parent.Close))
		self.range_min_value.KillFocus()
		self.range_min_value.Show()

		self.range_max_value = ui.EditLine()
		self.range_max_value.SetParent(self.range_img)
		self.range_max_value.SetPosition(50, 3)
		self.range_max_value.SetSize(36, 15)
		self.range_max_value.SetNumberMode()
		self.range_max_value.SetMax(3)
		self.range_max_value.SetText(str(self.max))
		self.range_max_value.SetEscapeEvent(ui.__mem_func__(root_parent.Close))
		self.range_min_value.KillFocus()
		self.range_max_value.Show()

		# Escape & Return Event
		self.range_min_value.SetReturnEvent(ui.__mem_func__(self.EditLineKillFocus))
		self.range_min_value.SetTabEvent(ui.__mem_func__(self.EditLineKillFocus))

		self.range_max_value.SetReturnEvent(ui.__mem_func__(self.EditLineKillFocus))
		self.range_max_value.SetTabEvent(ui.__mem_func__(self.EditLineKillFocus))

		# Mask
		if app.__BL_CLIP_MASK__:
			self.desc_title.SetClippingMaskWindow(parent)
			self.range_img.SetClippingMaskWindow(parent)

			self.range_min_value.SetClippingMaskWindow(parent)
			self.range_max_value.SetClippingMaskWindow(parent)

	def __del__(self):
		self.desc_title = None
		self.desc_title_text = None
		self.range_img = None
		self.range_min_value = None
		self.range_max_value = None

	def UpdatePosition(self, pos):
		self.desc_title.SetPosition(self.x, self.y - pos)
		self.range_img.SetPosition(self.x + 140, self.y - pos - 1)

	def Arrange(self, pos_x, pos_y):
		self.x = pos_x
		self.y = pos_y
		self.desc_title.SetPosition(self.x, self.y)
		self.range_img.SetPosition(self.x + 140, self.y - 1)

	def EditLineKillFocus(self):
		min_range_val = 0
		max_range_val = 0

		try:
			min_range_val = int(self.range_min_value.GetText())
			max_range_val = int(self.range_max_value.GetText())

			min_range_val = max(min_range_val, self.min)
			max_range_val = min(max_range_val, self.max)

		except:
			min_range_val = self.min
			max_range_val = self.max

		min_range_val = min(min_range_val, max_range_val)

		self.range_min_value.SetText(str(min_range_val))
		self.range_max_value.SetText(str(max_range_val))

		if self.range_min_value.IsFocus():
			self.range_min_value.KillFocus()
			# self.range_max_value.SetFocus()

		elif self.range_max_value.IsFocus():
			self.range_max_value.KillFocus()
			# self.range_min_value.SetFocus()

		else:
			pass

	def GetMinValue(self):
		text = self.range_min_value.GetText()

		if len(text) == 0:
			return self.min

		return int(text)

	def GetMaxValue(self):
		text = self.range_max_value.GetText()

		if len(text) == 0:
			return self.max

		return int(text)

	def SetMinValue(self, value):
		self.range_min_value.SetText(str(value))

	def SetMaxValue(self, value):
		self.range_max_value.SetText(str(value))

	def GetKey(self):
		return self.key

	def EditLineKillFocusAll(self):
		self.EditLineKillFocus()
		self.range_min_value.KillFocus()
		self.range_max_value.KillFocus()

	def GetHeight(self):
		return self.height

class Option:
	OPTION_HEIGHT = 16

	def __init__(self, parent, title, key):
		self.x = 0
		self.y = 0
		self.height = self.OPTION_HEIGHT

		self.option_title = ui.Button()
		self.option_title.SetParent(parent)
		self.option_title.SetEvent(ui.__mem_func__(self.__ToggleChecked))
		self.option_title.SetUpVisual(LOOTING_SYSTEM_IMG_PATH + "select_sub_title_bg.sub")
		self.option_title.SetOverVisual(LOOTING_SYSTEM_IMG_PATH + "select_sub_title_bg.sub")
		self.option_title.SetDownVisual(LOOTING_SYSTEM_IMG_PATH + "select_sub_title_bg.sub")
		self.option_title.SetText(title)
		self.option_title.Show()

		self.option_btn = ui.ImageBox()
		self.option_btn.SetParent(self.option_title)
		self.option_btn.AddFlag("not_pick")
		self.option_btn.SetPosition(105, 1)
		self.option_btn.Show()

		self.SetChecked(True)
		self.SetKey(key)

		# Mask
		if app.__BL_CLIP_MASK__:
			self.option_title.SetClippingMaskWindow(parent)

	def __del__(self):
		self.option_title = None
		self.option_btn = None

	def GetKey(self):
		return self.key

	def SetKey(self, key):
		self.key = key

	def SetChecked(self, is_checked):
		self.is_checked = is_checked
		if is_checked:
			self.option_btn.LoadImage(LOOTING_SYSTEM_IMG_PATH + "check_box.sub")
		else:
			self.option_btn.LoadImage(LOOTING_SYSTEM_IMG_PATH + "uncheck_box.sub")

	def __ToggleChecked(self):
		self.SetChecked(not self.is_checked)

	def IsChecked(self):
		return self.is_checked

	def UpdatePosition(self, pos):
		self.option_title.SetPosition(self.x, self.y - pos)

	def Arrange(self, pos_x, pos_y):
		self.x = pos_x
		self.y = pos_y
		self.option_title.SetPosition(self.x, self.y)

	def GetHeight(self):
		return self.height

class Category:
	TITLE_HEIGHT = 16

	def __init__(self, parent, title):
		self.parent = proxy(parent)

		self.x = 0
		self.y = 0
		self.max_y = 0
		self.height = self.TITLE_HEIGHT

		self.option_list = []
		self.range_list = []

		self.title = ui.ImageBox()
		self.title.SetParent(parent)
		self.title.AddFlag("not_pick")
		self.title.LoadImage(LOOTING_SYSTEM_IMG_PATH + "select_title_bg.sub")
		self.title.Show()

		self.title_text = ui.TextLine()
		self.title_text.SetParent(self.title)
		self.title_text.SetPosition(self.title.GetWidth() / 2, self.title.GetHeight() / 2)
		self.title_text.SetVerticalAlignCenter()
		self.title_text.SetHorizontalAlignCenter()
		self.title_text.SetText(title)
		self.title_text.Show()

		# Mask
		if app.__BL_CLIP_MASK__:
			self.title.SetClippingMaskWindow(parent)

	def __del__(self):
		self.option_list = []
		self.range_list = []

		self.title = None
		self.title_text = None

	def AddOption(self, title, key):
		self.option_list.append(Option(self.parent, title, key))

	def AddRange(self, root_parent, title, min, max, key = ""):
		self.range_list.append(Range(root_parent, self.parent, title, min, max, key))

	def GetOptions(self):
		return self.option_list

	def GetRanges(self):
		return self.range_list

	def GetPositionMaxY(self):
		return self.max_y

	def Arrange(self, pos_x, pos_y):
		self.x = pos_x + 4
		self.y = pos_y
		self.title.SetPosition(self.x, self.y)

		self.max_y = self.y
		to_left = True

		for v in self.option_list:
			if to_left:
				self.max_y += 18
				pos_x = self.x + 6
			else:
				pos_x = self.x + 140

			v.Arrange(pos_x, self.max_y)
			to_left = not to_left

		if len(self.range_list) > 0:
			self.max_y += 1

		for v in self.range_list:
			self.max_y += 19
			v.Arrange(self.x, self.max_y)

	def UpdatePosition(self, pos):
		self.title.SetPosition(self.x, self.y - pos)

		for v in self.option_list:
			v.UpdatePosition(pos)

		for v in self.range_list:
			v.UpdatePosition(pos)

	def GetHeight(self):
		height = self.height
		for index, option in enumerate(self.option_list):
			if index % 2 == 1:
				height += option.GetHeight()

		for range in self.range_list:
			height += range.GetHeight()

		return height

class Topic:
	TITLE_HEIGHT = 23

	def __init__(self, parent, title, tooltip_text = "", key = ""):
		self.parent = proxy(parent)

		self.x = 0
		self.y = 0
		self.height = self.TITLE_HEIGHT

		self.category_list = []

		self.key = key
		self.state = DEFAULT_ONOFF

		self.toolTip = uiToolTip.ToolTip()
		self.toolTip.AutoAppendTextLine(tooltip_text)
		self.toolTip.AlignHorizonalCenter()

		self.title_img = ui.ImageBox()
		self.title_img.SetParent(self.parent)
		self.title_img.AddFlag("not_pick")
		self.title_img.LoadImage(LOOTING_SYSTEM_IMG_PATH + "main_topic_title_bg.sub")
		self.title_img.Show()

		self.title_text = ui.TextLine()
		self.title_text.SetParent(self.title_img)
		self.title_text.SetPosition(self.title_img.GetWidth() / 2, self.title_img.GetHeight() / 2)
		self.title_text.SetVerticalAlignCenter()
		self.title_text.SetHorizontalAlignCenter()
		self.title_text.SetText(title)
		self.title_text.Show()

		self.info_btn = ui.ImageBox()
		self.info_btn.SetParent(self.title_img)
		self.info_btn.SetPosition(200, 3)
		self.info_btn.LoadImage("d:/ymir work/ui/pattern/q_mark_01.tga")
		self.info_btn.SetEvent(ui.__mem_func__(self.EventProgress), "mouse_over_in")
		self.info_btn.SetEvent(ui.__mem_func__(self.EventProgress), "mouse_over_out")
		self.info_btn.Show()

		self.state_btn = ui.ToggleButton()
		self.state_btn.SetParent(self.parent)
		self.state_btn.SetUpVisual("d:/ymir work/ui/public/small_button_01.sub")
		self.state_btn.SetOverVisual("d:/ymir work/ui/public/small_button_02.sub")
		self.state_btn.SetDownVisual("d:/ymir work/ui/public/small_button_03.sub")
		self.state_btn.SetToggleUpEvent(ui.__mem_func__(self.__StateButton), False)
		self.state_btn.SetToggleDownEvent(ui.__mem_func__(self.__StateButton), True)
		self.state_btn.SetText(uiScriptLocale.LOOTING_SYSTEM_ON)
		self.state_btn.Down()
		self.state_btn.Show()

		# Mask
		if app.__BL_CLIP_MASK__:
			self.title_img.SetClippingMaskWindow(self.parent)
			self.state_btn.SetClippingMaskWindow(self.parent)

	def EventProgress(self, event_type):
		if self.toolTip is None:
			return

		if "mouse_over_out" == event_type:
			self.toolTip.HideToolTip()

		elif "mouse_over_in" == event_type:
			self.toolTip.ShowToolTip()

	def __del__(self):
		self.category_list = {}

		self.title_img = None
		self.title_text = None
		self.info_btn = None
		self.state_btn = None

		self.toolTip = None

	def __StateButton(self, enable):
		self.state_btn.SetText(uiScriptLocale.LOOTING_SYSTEM_ON if enable else uiScriptLocale.LOOTING_SYSTEM_OFF)
		self.state = enable

	def GetKey(self):
		return self.key

	def GetState(self):
		return self.state

	def SetState(self, enable):
		self.__StateButton(enable)
		self.state_btn.Down() if enable else self.state_btn.SetUp()

	def AddCategory(self, category):
		self.category_list.append(category)

	def GetCategory(self):
		return self.category_list

	def GetPositionY(self):
		pos_y = 0
		if len(self.category_list) > 0:
			pos_y = self.category_list[-1].GetPositionMaxY() + 25

		return pos_y

	def Arrange(self, pos_x, pos_y):
		self.y = pos_y
		self.title_img.SetPosition(self.x, self.y)
		self.state_btn.SetPosition(self.x + 225, self.y + 2)

		pos_y = self.y + 30
		for v in self.category_list:
			v.Arrange(pos_x, pos_y)
			pos_y = v.GetPositionMaxY() + 20

	def UpdatePosition(self, pos):
		if self.title_img:
			self.title_img.SetPosition(self.x, self.y - pos)

		if self.state_btn:
			self.state_btn.SetPosition(self.x + 225, self.y + 2 - pos)

		for v in self.category_list:
			v.UpdatePosition(pos)

	def GetHeight(self):
		height = self.height
		for category in self.category_list:
			height += category.GetHeight()
			height += 2 # gap
		return height

class LootingSystem(ui.ScriptWindow):
	def __init__(self):
		ui.ScriptWindow.__init__(self)
		self.isLoaded = False

		self.topic_dict = []

		self.scroll_bar = None
		self.mask_window = None

		self.filter_dict = {}

		self.close_question_dialog = None
		self.init_question_dialog = None
		self.save_question_dialog = None

		if app.ENABLE_PREMIUM_LOOT_FILTER:
			self.message_bg = None
			self.message_text = None

		self.__LoadWindow()

	def __del__(self):
		ui.ScriptWindow.__del__(self)

		self.topic_dict = []

		self.scroll_bar = None
		self.mask_window = None

		self.filter_dict = {}

		self.close_question_dialog = None
		self.init_question_dialog = None
		self.save_question_dialog = None

		if app.ENABLE_PREMIUM_LOOT_FILTER:
			self.message_bg = None
			self.message_text = None

	def __LoadWindow(self):
		if self.isLoaded:
			return

		self.isLoaded = True

		# Load Script
		try:
			pyScrLoader = ui.PythonScriptLoader()
			pyScrLoader.LoadScriptFile(self, "UIScript/LootingSystem.py")
		except:
			import exception
			exception.Abort("LootingSystem.__LoadWindow.LoadScript")

		# Bind Objects
		try:
			self.__BindObject()
		except:
			import exception
			exception.Abort("LootingSystem.__LoadWindow.__BindObject")

		# Bind Events
		try:
			self.__BindEvent()
		except:
			import exception
			exception.Abort("LootingSystem.__LoadWindow.__BindEvent")

		# Create Objects
		try:
			self.__CreateObject()
		except:
			import exception
			exception.Abort("LootingSystem.__LoadWindow.__CreateObject")

		# Load File
		try:
			self.__LoadSettings()
		except:
			import exception
			exception.Abort("LootingSystem.__LoadWindow.__LoadSettings")

	def __BindObject(self):
		if app.ENABLE_PREMIUM_LOOT_FILTER:
			self.board = self.GetChild("board")
		self.scroll_bar = self.GetChild("scroll_bar")
		self.mask_window = self.GetChild("object_area_window")
		self.init_button = self.GetChild("init_button")
		self.save_button = self.GetChild("save_button")

	def __BindEvent(self):
		self.GetChild("board").SetCloseEvent(ui.__mem_func__(self.Close))
		self.scroll_bar.SetScrollEvent(ui.__mem_func__(self.__ScrollEvent))
		self.scroll_bar.SetScrollStep(SCROLL_STEP)

	def __CreateObject(self):
		## Weapon
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_WEAPON, uiScriptLocale.LOOTING_SYSTEM_QUESTION_WEAPON,"weapon")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_WARRIOR, "warrior")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_SURA, "sura")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_ASSASSIN, "assassin")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_SHAMAN, "shaman")
		if app.ENABLE_WOLFMAN_CHARACTER:
			category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_WOLFMAN, "wolfman")
		category.AddRange(self, uiScriptLocale.LOOTING_SYSTEM_REFINE, REFINE_MIN, REFINE_MAX, "refine")
		category.AddRange(self, uiScriptLocale.LOOTING_SYSTEM_WEARING_LEVEL, WEARING_LEVEL_MIN, WEARING_LEVEL_MAX, "wearing_level")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Armor
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_ARMOR, uiScriptLocale.LOOTING_SYSTEM_QUESTION_ARMOR, "armor")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_WARRIOR, "warrior")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_SURA, "sura")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_ASSASSIN, "assassin")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_SHAMAN, "shaman")
		if app.ENABLE_WOLFMAN_CHARACTER:
			category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_WOLFMAN, "wolfman")
		category.AddRange(self, uiScriptLocale.LOOTING_SYSTEM_REFINE, REFINE_MIN, REFINE_MAX, "refine")
		category.AddRange(self, uiScriptLocale.LOOTING_SYSTEM_WEARING_LEVEL, WEARING_LEVEL_MIN, WEARING_LEVEL_MAX, "wearing_level")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Head
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_HEAD, uiScriptLocale.LOOTING_SYSTEM_QUESTION_HEAD, "head")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_WARRIOR, "warrior")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_SURA, "sura")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_ASSASSIN, "assassin")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_SHAMAN, "shaman")
		if app.ENABLE_WOLFMAN_CHARACTER:
			category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_WOLFMAN, "wolfman")
		category.AddRange(self, uiScriptLocale.LOOTING_SYSTEM_REFINE, REFINE_MIN, REFINE_MAX, "refine")
		category.AddRange(self, uiScriptLocale.LOOTING_SYSTEM_WEARING_LEVEL, WEARING_LEVEL_MIN, WEARING_LEVEL_MAX, "wearing_level")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Common
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_COMMON, uiScriptLocale.LOOTING_SYSTEM_QUESTION_COMMON, "common")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_FOOTS, "foots")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_BELT, "belt")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_WRIST, "wrist")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_NECK, "neck")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_EAR, "ear")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_SHIELD, "shield")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_GLOVE, "glove")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_PENDANT, "pendant")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_ROD, "rod")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COMMON_PICK, "pick")
		category.AddRange(self, uiScriptLocale.LOOTING_SYSTEM_REFINE, REFINE_MIN, REFINE_MAX, "refine")
		category.AddRange(self, uiScriptLocale.LOOTING_SYSTEM_WEARING_LEVEL, WEARING_LEVEL_MIN, WEARING_LEVEL_MAX, "wearing_level")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Costume
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_COSTUME, uiScriptLocale.LOOTING_SYSTEM_QUESTION_COSTUME, "costume")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COSTUME)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COSTUME_WEAPON, "weapon")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COSTUME_ARMOR, "armor")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COSTUME_HAIR, "hair")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COSTUME_ACCE, "acce")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COSTUME_AURA, "aura")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_COSTUME_ETC, "etc")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Dragon Soul
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_DS, uiScriptLocale.LOOTING_SYSTEM_QUESTION_DS, "ds")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_DS)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_DS_DS, "ds")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_DS_ETC, "etc")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Unique
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_UNIQUE, uiScriptLocale.LOOTING_SYSTEM_QUESTION_UNIQUE, "unique")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_UNIQUE)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_UNIQUE_ABILITY, "ability")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_UNIQUE_ETC, "etc")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Refine
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_REFINE, uiScriptLocale.LOOTING_SYSTEM_QUESTION_REFINE, "refine")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_REFINE)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_REFINE_MATERIAL, "material")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_REFINE_STONE, "stone")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_REFINE_ETC, "etc")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Potion
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_POTION, uiScriptLocale.LOOTING_SYSTEM_QUESTION_POTION, "potion")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_POTION)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_POTION_ABILITY, "ability")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_POTION_HAIRDYE, "hairdye")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_POTION_ETC, "etc")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Fish & Mining
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_FISH_MINING, uiScriptLocale.LOOTING_SYSTEM_QUESTION_FISH_MINING, "fish_mining")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_FISH_MINING)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_FISH_MINING_FOOD, "food")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_FISH_MINING_STONE, "stone")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_FISH_MINING_ETC, "etc")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Mount & Pet
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_MOUNT_PET_MOUNT, uiScriptLocale.LOOTING_SYSTEM_QUESTION_MOUNT_PET, "mount_pet")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_MOUNT_PET)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_MOUNT_PET_CHARGED_PET, "charged_pet")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_MOUNT_PET_MOUNT, "mount")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_MOUNT_PET_FREE_PET, "free_pet")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_MOUNT_PET_EGG, "egg")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Skill Book
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_SKILL_BOOK, uiScriptLocale.LOOTING_SYSTEM_QUESTION_SKILL_BOOK, "skill_book")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_WARRIOR, "warrior")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_SURA, "sura")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_ASSASSIN, "assassin")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_SHAMAN, "shaman")
		if app.ENABLE_WOLFMAN_CHARACTER:
			category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_WOLFMAN, "wolfman")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_JOB_PUBLIC, "public")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Etc.
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_ETC, uiScriptLocale.LOOTING_SYSTEM_QUESTION_ETC, "etc")
		category = Category(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_CATEGORY_ETC)
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_ETC_GIFTBOX, "giftbox")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_ETC_MATRIMONY, "matrimony")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_ETC_SEAL, "seal")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_ETC_PARTY, "party")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_ETC_POLYMORPH, "polymorph")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_ETC_RECIPE, "recipe")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_ETC_WEAPON_ARROW, "weapon_arrow")
		category.AddOption(uiScriptLocale.LOOTING_SYSTEM_CATEGORY_ETC_ETC, "etc")
		topic.AddCategory(category)
		self.topic_dict.append(topic)

		## Event & Other
		topic = Topic(self.mask_window, uiScriptLocale.LOOTING_SYSTEM_EVENT, uiScriptLocale.LOOTING_SYSTEM_QUESTION_EVENT, "event")
		self.topic_dict.append(topic)

		# Scroll Size
		pos_y = 0
		height = 0
		for topic in self.topic_dict:
			topic.Arrange(0, pos_y)
			pos_y = topic.GetPositionY()
			height += topic.GetHeight()

		self.scroll_value = height

		## Initialize Button / Reset
		if self.init_button:
			self.init_button.SetEvent(ui.__mem_func__(self.__OnClickInitButton))

		## Save Button
		if self.save_button:
			self.save_button.SetEvent(ui.__mem_func__(self.__OnClickSaveButton))

		if app.ENABLE_PREMIUM_LOOT_FILTER:
			## Adjust Board Size for Message Background
			self.board.SetSize(self.board.GetWidth(), self.board.GetHeight() + 30)

			## Message Background
			message_bg = ui.ImageBox()
			message_bg.SetParent(self.board)
			message_bg.LoadImage(LOOTING_SYSTEM_IMG_PATH + "message_bg.sub")
			message_bg.AddFlag("not_pick")
			message_bg.SetSize(280, 18)
			message_bg.SetWindowHorizontalAlignCenter()
			message_bg.SetWindowVerticalAlignBottom()
			message_bg.SetPosition(0, 35)
			message_bg.Show()
			self.message_bg = message_bg

			## Message Text
			message_text = ui.TextLine()
			message_text.SetParent(self.message_bg)
			message_text.SetPosition(0, 2)
			message_text.SetText(uiScriptLocale.LOOTING_SYSTEM_MESSAGE_ENABLE_VOUCHER)
			message_text.SetWindowHorizontalAlignCenter()
			message_text.SetHorizontalAlignCenter()
			message_text.Show()
			self.message_text = message_text

	def __ScrollEvent(self):
		self.EditLineKillFocusAll()

		pos = self.scroll_bar.GetPos() * self.scroll_value
		for v in self.topic_dict:
			v.UpdatePosition(int(pos))

	def EditLineKillFocusAll(self):
		for topic in self.topic_dict:
			for category in topic.category_list:
				for range in category.range_list:
					range.EditLineKillFocusAll()

	def OnPressEscapeKey(self):
		self.Hide()
		return True

	def Open(self):
		if self.IsShow():
			return

		self.__LoadSettings()
		self.Show()
		self.SetTop()

		if app.__BL_MOUSE_WHEEL_TOP_WINDOW__:
			wndMgr.SetWheelTopWindow(self.hWnd)

		if app.ENABLE_PREMIUM_LOOT_FILTER:
			self.__RefreshMessageText()

	def Close(self):
		self.__OpenCloseQuestionDialog()
		self.EditLineKillFocusAll()

	def __GetFile(self):
		pathlist = [os.getcwd(), "data", "looting"]
		path = os.sep.join(pathlist)

		try:
			if not os.path.exists(path):
				os.makedirs(path)
		except WindowsError as error:
			pass

		return "/".join(pathlist[1:] + [player.GetName()])

	def __LoadSettings(self):
		item.SaveLootingSettings("{}")
		try:
			filename = self.__GetFile()
			if os.path.exists(filename):
				with old_open(filename, "rb") as f:
					self.filter_dict = json.load(f)
					item.SaveLootingSettings(json.dumps(self.filter_dict))
		except (AttributeError, IOError, EOFError, ValueError):
			self.__InitTopic()
			return

		for topic in self.topic_dict:
			topic_key = topic.GetKey()

			# Create the topic key in the dict if it doesn't exists.
			if topic_key and topic_key in self.filter_dict.keys():
				topic_settings = self.filter_dict[topic_key]
				topic.SetState(topic_settings["onoff"])

				for category in topic.GetCategory():

					# Settings for each category button fields.
					if "select" in topic_settings.keys():
						for i, option in enumerate(category.GetOptions()):
							key = option.GetKey()
							if key in topic_settings["select"]:
								option.SetChecked(topic_settings["select"][key])

					# Settings for each category range fields.
					for range in category.GetRanges():
						range_key = range.GetKey()

						if range_key == "refine":
							range.SetMinValue(topic_settings["refine_min"] if "refine_min" in topic_settings.keys() else range.min)
							range.SetMaxValue(topic_settings["refine_max"] if "refine_max" in topic_settings.keys() else range.max)

						if range_key == "wearing_level":
							range.SetMinValue(topic_settings["wearing_level_min"] if "wearing_level_min" in topic_settings.keys() else range.min)
							range.SetMaxValue(topic_settings["wearing_level_max"] if "wearing_level_max" in topic_settings.keys() else range.max)

						else:
							pass

	## Reset Topics / Settings
	def __OnClickInitButton(self):
		# Check if any question dialog is already open.
		if self.close_question_dialog:
			self.close_question_dialog.SetTop()
			return

		if self.save_question_dialog:
			self.save_question_dialog.SetTop()
			return

		questionDialog = uiCommon.QuestionDialog()
		questionDialog.SetText(uiScriptLocale.LOOTING_SYSTEM_INIT_QUESTION_DIALOG_TITLE)
		questionDialog.SetAcceptEvent(ui.__mem_func__(self.__InitAccept))
		questionDialog.SetCancelEvent(ui.__mem_func__(self.__InitCancel))
		questionDialog.Open()
		self.init_question_dialog = questionDialog

	def __InitTopic(self):
		# Reset all fields with default values.
		for topic in self.topic_dict:
			topic.SetState(DEFAULT_ONOFF)

			for category in topic.GetCategory():
				for option in category.GetOptions():
					option.SetChecked(DEFAULT_ONOFF)

				for range in category.GetRanges():
					range.SetMinValue(range.min)
					range.SetMaxValue(range.max)

	def __InitAccept(self):
		if not self.init_question_dialog:
			return

		self.__InitTopic()
		self.__InitCancel()

	def __InitCancel(self):
		self.init_question_dialog.Close()
		self.init_question_dialog = None

	## Save Topics / Settings
	def __OnClickSaveButton(self):
		# Check if any question dialog is already open.
		if self.init_question_dialog:
			self.init_question_dialog.SetTop()
			return

		if self.close_question_dialog:
			self.close_question_dialog.SetTop()
			return

		if app.ENABLE_PREMIUM_LOOT_FILTER:
			if not player.CheckAffect(chr.NEW_AFFECT_LOOTING_SYSTEM, 0):
				chat.AppendChat(chat.CHAT_TYPE_INFO, uiScriptLocale.LOOTING_SYSTEM_MESSAGE_ENABLE_VOUCHER)
				return

		questionDialog = uiCommon.QuestionDialog()
		questionDialog.SetText(uiScriptLocale.LOOTING_SYSTEM_EXIT_WITH_SAVING)
		questionDialog.SetAcceptEvent(ui.__mem_func__(self.__SaveAccept))
		questionDialog.SetCancelEvent(ui.__mem_func__(self.__SaveCancel))
		questionDialog.Open()
		self.save_question_dialog = questionDialog

	def __GetData(self):
		self.filter_dict.clear()

		for topic in self.topic_dict:
			topic_key = topic.GetKey()

			# Create the topic key in the dict if it doesn't exists.
			if topic_key and not topic_key in self.filter_dict.keys():
				self.filter_dict[topic_key] = dict()

			topic_settings = self.filter_dict[topic_key]
			topic_settings["onoff"] = topic.GetState()

			for category in topic.GetCategory():

				# Settings for each category button fields.
				for option in category.GetOptions():
					if not "select" in topic_settings.keys():
						topic_settings["select"] = dict()
					topic_settings["select"][option.GetKey()] = option.IsChecked()

				# Settings for each category range text fields.
				for range in category.GetRanges():
					range_key = range.GetKey()

					if range_key == "refine":
						topic_settings["refine_min"] = range.GetMinValue()
						topic_settings["refine_max"] = range.GetMaxValue()

						topic_settings["refine_min"] = max(range.min, topic_settings["refine_min"])
						topic_settings["refine_max"] = min(range.max, topic_settings["refine_max"])

						topic_settings["refine_min"] = min(topic_settings["refine_min"], topic_settings["refine_max"])
						topic_settings["refine_max"] = max(topic_settings["refine_min"], topic_settings["refine_max"])

						range.SetMinValue(topic_settings["refine_min"])
						range.SetMaxValue(topic_settings["refine_max"])

					if range_key == "wearing_level":
						topic_settings["wearing_level_min"] = range.GetMinValue()
						topic_settings["wearing_level_max"] = range.GetMaxValue()

						topic_settings["wearing_level_min"] = max(range.min, topic_settings["wearing_level_min"])
						topic_settings["wearing_level_max"] = min(range.max, topic_settings["wearing_level_max"])

						topic_settings["wearing_level_min"] = min(topic_settings["wearing_level_min"], topic_settings["wearing_level_max"])
						topic_settings["wearing_level_max"] = max(topic_settings["wearing_level_min"], topic_settings["wearing_level_max"])

						range.SetMinValue(topic_settings["wearing_level_min"])
						range.SetMaxValue(topic_settings["wearing_level_max"])

		return self.filter_dict

	def __SaveFile(self):
		with old_open(self.__GetFile(), 'wb') as f:
			data = self.__GetData()
			json.dump(data, f, indent=4)
			item.SaveLootingSettings(json.dumps(data))

	def __SaveAccept(self):
		if not self.save_question_dialog:
			return

		self.__SaveCancel()
		self.__SaveFile()

		self.Hide()

	def __SaveCancel(self):
		self.save_question_dialog.Close()
		self.save_question_dialog = None

	## Close Board
	def __OpenCloseQuestionDialog(self):
		# Check if any question dialog is already open.
		if self.init_question_dialog:
			self.init_question_dialog.SetTop()
			return

		if self.save_question_dialog:
			self.save_question_dialog.SetTop()
			return

		questionDialog = uiCommon.QuestionDialog()
		questionDialog.SetText(uiScriptLocale.LOOTING_SYSTEM_EXIT_WITHOUT_SAVING)
		questionDialog.SetAcceptEvent(ui.__mem_func__(self.__CloseAccept))
		questionDialog.SetCancelEvent(ui.__mem_func__(self.__CloseCancel))
		questionDialog.Open()
		self.close_question_dialog = questionDialog

	def __CloseAccept(self):
		if not self.close_question_dialog:
			return

		self.__CloseCancel()

		self.Hide()

		# if app.__BL_MOUSE_WHEEL_TOP_WINDOW__:
		# 	wndMgr.ClearWheelTopWindow()

	def __CloseCancel(self):
		self.close_question_dialog.Close()
		self.close_question_dialog = None

	if app.ENABLE_PREMIUM_LOOT_FILTER:
		def __RefreshMessageText(self):
			if not player.CheckAffect(chr.NEW_AFFECT_LOOTING_SYSTEM, 0):
				self.message_text.SetText(uiScriptLocale.LOOTING_SYSTEM_MESSAGE_ENABLE_VOUCHER)
			else:
				self.message_text.SetText(uiScriptLocale.LOOTING_SYSTEM_MESSAGE_POSSIBLE_ITEM_PICKUP)

	def OnMouseWheel(self, length):
		if self.scroll_bar:
			if length>0:
				self.scroll_bar.OnUp()
			else:
				self.scroll_bar.OnDown()
			return True
		return False

	if app.__BL_MOUSE_WHEEL_TOP_WINDOW__:
		def OnMouseWheelButtonUp(self):
			if self.scroll_bar:
				self.scroll_bar.OnUp()
				return True

			return False

		def OnMouseWheelButtonDown(self):
			if self.scroll_bar:
				self.scroll_bar.OnDown()
				return True

			return False
