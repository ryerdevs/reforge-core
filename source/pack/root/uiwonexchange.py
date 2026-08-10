import ui
import localeInfo
import uiCommon
import uiScriptLocale
import app
import net
from _weakref import proxy

class WonExchangeWindow(ui.ScriptWindow):
	WON_NAME_VALUE = 100000000
	TAX_NAME_MUL = 0
	PAGE_BUY, PAGE_SELL = range(2)

	def __init__(self):
		ui.ScriptWindow.__init__(self)
		self.page = self.PAGE_BUY
		self.isLoaded = 0

		self.__Initialize()
		self.__LoadWindow()

	def __del__(self):
		ui.ScriptWindow.__del__(self)

	def Destroy(self):
		self.ClearDictionary()
		self.__Initialize()

	def __Initialize(self):
		self.interface = None
		self.tabDict = None
		self.tabButtonDict = None
		self.pageDict = None
		self.titleBarDict = None
		self.inputWon = None
		self.resultGold = None
		self.dlgQuestion = None

	def __LoadWindow(self):
		if self.isLoaded == 1:
			return

		self.isLoaded = 1

		try:
			pyScrLoader = ui.PythonScriptLoader()
			pyScrLoader.LoadScriptFile(self, "UIScript/WonExchangeWindow.py")
		except:
			import exception
			exception.Abort("WonExchangeWindow.__LoadWindow.LoadScriptFile.UIScript/WonExchangeWindow.py")

		try:
			self.__BindObject()
		except:
			import exception
			exception.Abort("WonExchangeWindow.__LoadWindow.__BindObject.UIScript/WonExchangeWindow.py")

		try:
			self.__BindEvent()
		except:
			import exception
			exception.Abort("WonExchangeWindow.__LoadWindow.__BindEvent.UIScript/WonExchangeWindow.py")

		self.SetPage(self.PAGE_SELL)

	def __BindObject(self):
		self.tabDict = {
			self.PAGE_SELL	: self.GetChild("Tab_01"),
			self.PAGE_BUY	: self.GetChild("Tab_02"),
		}

		self.tabButtonDict = {
			self.PAGE_SELL	: self.GetChild("Tab_Button_01"),
			self.PAGE_BUY	: self.GetChild("Tab_Button_02"),
		}

		self.pageDict = {
			self.PAGE_BUY	: self.GetChild("CurrencyConverter_Page"),
			self.PAGE_SELL	: self.GetChild("CurrencyConverter_Page"),
		}

		self.titleBarDict = {
			self.PAGE_BUY	: self.GetChild("BuyWon_TitleBar"),
			self.PAGE_SELL	: self.GetChild("SellWon_TitleBar"),
		}

		self.inputWon = self.GetChild("Input")
		self.inputWon.SetEscapeEvent(ui.__mem_func__(self.Close))
		self.resultGold = self.GetChild("Result")
		self.acceptButton = self.GetChild("AcceptButton")

		if localeInfo.IsARABIC():
			for tab in self.tabDict.itervalues():
				tab.SetScale(-1.0, 1.0)
				tab.SetRenderingRect(-1.0, 0.0, 1.0, 0.0)

		self.dlgQuestion = uiCommon.QuestionDialog2()
		self.dlgQuestion.Close()

	def __BindEvent(self):
		for (tabKey, tabButton) in self.tabButtonDict.items():
			tabButton.SetEvent(ui.__mem_func__(self.__OnClickTabButton), tabKey)

		for titleBarValue in self.titleBarDict.itervalues():
			titleBarValue.SetCloseEvent(ui.__mem_func__(self.Close))

		self.inputWon.OnIMEUpdate = ui.__mem_func__(self.__CustomeIMEUpdate)
		self.acceptButton.SetEvent(ui.__mem_func__(self.__OpenQuestionDialog))

	def __CustomeIMEUpdate(self):
		ui.EditLine.OnIMEUpdate(self.inputWon)
		try:
			self.resultGold.SetText("%s" % (localeInfo.MoneyFormat(long(((1.0 + self.TAX_NAME_MUL / 100.0) if self.page == self.PAGE_BUY else 1.0) * self.WON_NAME_VALUE) * long(self.inputWon.GetText()))))
		except:
			self.resultGold.SetText("")

	def ClearCurrencyConverterPage(self, isFocus):
		self.inputWon.SetText("")
		self.resultGold.SetText("")
		if isFocus:
			self.inputWon.SetFocus()
		else:
			self.inputWon.KillFocus()

	def __OnClickTabButton(self, pageKey):
		self.dlgQuestion.Close()
		self.ClearCurrencyConverterPage(True)
		self.SetPage(pageKey)

	def Open(self):
		self.__OnClickTabButton(self.PAGE_SELL)
		self.Show()

	def SetPage(self, pageKey):
		self.page = pageKey

		for (tabKey, tabButton) in self.tabButtonDict.items():
			if pageKey!=tabKey:
				tabButton.SetUp()

		for tabValue in self.tabDict.itervalues():
			tabValue.Hide()

		for pageValue in self.pageDict.itervalues():
			pageValue.Hide()

		for titleBarValue in self.titleBarDict.itervalues():
			titleBarValue.Hide()

		self.titleBarDict[pageKey].Show()
		self.tabDict[pageKey].Show()
		self.pageDict[pageKey].Show()

	def __OpenQuestionDialog(self):
		if not self.inputWon.GetText():
			return

		self.dlgQuestion.SetAcceptEvent(ui.__mem_func__(self.__Accept))
		self.dlgQuestion.SetCancelEvent(ui.__mem_func__(self.__Cancel))

		args = (int(self.inputWon.GetText()), localeInfo.MoneyFormat(long(((1.0 + self.TAX_NAME_MUL / 100.0) if self.page == self.PAGE_BUY else 1.0) * self.WON_NAME_VALUE) * long(self.inputWon.GetText())))
		if self.page == self.PAGE_SELL:
			self.dlgQuestion.SetText1(localeInfo.WONEXCHANGE_CONFIRM_QUESTION_1 % args)
		elif self.page == self.PAGE_BUY:
			self.dlgQuestion.SetText1(localeInfo.WONEXCHANGE_CONFIRM_QUESTION_2 % args)
		self.dlgQuestion.SetText2(localeInfo.WONEXCHANGE_CONFIRM_QUESTION_3)
		self.dlgQuestion.Open()

	def __Accept(self):
		if not self.inputWon.GetText():
			return

		self.dlgQuestion.Close()

		if self.page == self.PAGE_SELL:
			net.SendChatPacket("/won_exchange sell {}".format(self.inputWon.GetText()))
		elif self.page == self.PAGE_BUY:
			net.SendChatPacket("/won_exchange buy {}".format(self.inputWon.GetText()))

		self.ClearCurrencyConverterPage(True)

	def __Cancel(self):
		self.dlgQuestion.Close()

	def BindInterface(self, interface):
		self.interface = proxy(interface)

	def Close(self):
		if self.dlgQuestion.IsShow():
			self.dlgQuestion.Close()

		for tries in xrange(3): #the focus queue may set again the previous editline (minimum 2 tries)
			if self.inputWon:
				self.inputWon.KillFocus()
			self.KillFocus()
		self.Hide()

	def ExternQuestionDialog_Close(self):
		if self.dlgQuestion.IsShow():
			self.dlgQuestion.Close()

	def IsDlgQuestionShow(self):
		return self.dlgQuestion and self.dlgQuestion.IsShow()

	def OnPressEscapeKey(self):
		self.Close()
		return True
