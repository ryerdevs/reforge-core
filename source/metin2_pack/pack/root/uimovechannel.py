import app
import dbg
import ui
import exception
import localeInfo
import chat
import serverInfo
import net

def GetServerID():
	serverID = 0
	for k in serverInfo.REGION_DICT[0].keys():
		if serverInfo.REGION_DICT[0][k]["name"] == net.GetServerInfo().split(",")[0]:
			serverID = k
			break
	return serverID

class MoveChannelWindow(ui.ScriptWindow):
	def __init__(self):
		ui.ScriptWindow.__init__(self)

		self.__Initialize()
		self.__LoadWindow()

	def __del__(self):
		ui.ScriptWindow.__del__(self)

	def __Initialize(self):
		self.titleBar = None
		self.channelButtonList = []
		self.currentChannel = 0

		self.ingameChannel = 1

		self.board = None
		self.titleBar = None
		self.blackBoard = None
		self.acceptButton = None
		self.cancelButton = None

	@ui.WindowDestroy
	def Destroy(self):
		self.__Initialize()
		self.ClearDictionary()
		self.Hide()

	def Open(self):
		if self.ingameChannel < 99:
			self.SelectChannel(self.ingameChannel - 1)
		else:
			self.__RefreshChannelButtons()
		self.SetCenterPosition()
		self.Show()

	def Close(self):
		self.Hide()

	def OnPressEscapeKey(self):
		self.Close()
		return True

	def __LoadWindow(self):
		# run once
		if getattr(self, "IsLoaded", False):
			return
		self.IsLoaded = True

		# load ui
		try:
			pyScrLoader = ui.PythonScriptLoader()
			pyScrLoader.LoadScriptFile(self, "UIScript/MoveChannelDialog.py")
		except:
			exception.Abort("MoveChannelWindow.__LoadWindow.LoadScript")

		# getters
		try:
			self.board = self.GetChild("MoveChannelBoard")
			self.titleBar = self.GetChild("MoveChannelTitle")
			self.blackBoard = self.GetChild("BlackBoard")
			self.acceptButton = self.GetChild("AcceptButton")
			self.cancelButton = self.GetChild("CancelButton")
		except:
			exception.Abort("MoveChannelWindow.__LoadWindow.BindObject")

		# events
		self.titleBar.SetCloseEvent(ui.__mem_func__(self.Close))
		self.acceptButton.SetEvent(ui.__mem_func__(self.ChangeChannel))
		self.cancelButton.SetEvent(ui.__mem_func__(self.Close))

		# dynamic buttons
		self.__AddChannelButtons()

	def GetChannelCount(self):
		return len(serverInfo.REGION_DICT[0][GetServerID()]["channel"])

	def __AddChannelButtons(self):
		ELEM_SIZE = 30
		BOARD_SIZE = ELEM_SIZE * self.GetChannelCount()
		self.SetSize(190, 80 + BOARD_SIZE)
		self.board.SetSize(190, 80 + BOARD_SIZE)
		self.blackBoard.SetSize(163, 7 + BOARD_SIZE)

		for i in xrange(self.GetChannelCount()):
			self.channelButtonList.append(ui.MakeRadioButton(self.blackBoard, 7, 7 + ELEM_SIZE * i, "d:/ymir work/ui/game/myshop_deco/", "select_btn_01.sub", "select_btn_02.sub", "select_btn_03.sub"))
			self.channelButtonList[i].SetText(serverInfo.REGION_DICT[0][GetServerID()]["channel"][i]["name"])
			self.channelButtonList[i].SetEvent(lambda arg=i: self.SelectChannel(arg))
			self.channelButtonList[i].Show()

	def __RefreshChannelButtons(self):
		for i in xrange(self.GetChannelCount()):
			if i == self.currentChannel:
				self.channelButtonList[i].Down()
			else:
				self.channelButtonList[i].SetUp()

	def SelectChannel(self,channel):
		self.currentChannel = channel
		self.__RefreshChannelButtons()

	def ChangeChannel(self):
		channelID = self.currentChannel + 1
		if channelID <= 0: #skip invalid channel ids
			chat.AppendChat(chat.CHAT_TYPE_INFO , localeInfo.MOVE_CHANNEL_NOT_MOVE)
			return

		self.Close()
		net.SendChatPacket("/change_channel {}".format(channelID))
