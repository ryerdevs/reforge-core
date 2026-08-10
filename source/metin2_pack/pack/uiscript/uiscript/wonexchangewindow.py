import app
import uiScriptLocale

ROOT_PATH = "d:/ymir work/ui/game/wonexchange/"

BOARD_ADD_X = 0
BOARD_ADD_X += 50 # won
# BOARD_ADD_X += 60 # gaya
BOARD_ADD_X += 26 # wonexchange
BOARD_X = SCREEN_WIDTH - (140 + BOARD_ADD_X)

window = {
	"name" : "WonExchangeWindow", "style" : ("movable", "float", "ltr"),
	"x" : BOARD_X, "y" : SCREEN_HEIGHT - 65 - 150,
	"width" : 197, "height" : 150,
	"children" :
	(
		{
			"name" : "board", "type" : "board", "style" : ("attach",),
			"x" : 0, "y" : 0,
			"width" : 197, "height" : 150,
			"children" :
			(
				{
					"name" : "SellWon_TitleBar", "type" : "titlebar", "style" : ("attach",),
					"x" : 6, "y" : 7,
					"width" : 184, "color" : "red",
					"children" :
					(
						{ "name":"TitleName", "type":"text", "x":0, "y":-1, "text":uiScriptLocale.WONEXCHANGE_TITLE_SELL, "all_align":"center" },
					),
				},
				{
					"name" : "BuyWon_TitleBar", "type" : "titlebar", "style" : ("attach",),
					"x" : 6, "y" : 7,
					"width" : 184, "color" : "red",
					"children" :
					(
						{ "name":"TitleName", "type":"text", "x":0, "y":-1, "text":uiScriptLocale.WONEXCHANGE_TITLE_BUY, "all_align":"center" },
					),
				},
				## Tab Area
				{
					"name" : "TabControl",
					"type" : "window",

					"x" : 0, "y" : 121,
					"width" : 196, "height" : 36,
					"children" :
					(
						## Tab
						{
							"name" : "Tab_01", "type" : "expanded_image",
							"x" : 0, "y" : 0,
							"width" : 196, "height" : 36,
							"image" : ROOT_PATH+"tab_21.sub",
						},
						{
							"name" : "Tab_02", "type" : "expanded_image",
							"x" : 0, "y" : 0,
							"width" : 196, "height" : 36,
							"image" : ROOT_PATH+"tab_22.sub",
						},
						## RadioButton
						{
							"name" : "Tab_Button_01", "type" : "radio_button",
							"x" : 3, "y" : 3,
							"width" : 64, "height" : 30,
						},
						{
							"name" : "Tab_Button_02", "type" : "radio_button",
							"x" : 67, "y" : 3,
							"width" : 64, "height" : 30,
						},
					),
				},
				## Page Area
				{
					"name" : "CurrencyConverter_Page", "type" : "window", "style" : ("attach",),
					"x" : 0, "y" : 24,
					"width" : 192, "height" : 93,
					"children" :
					(
						{
							"name" : "CThinBoard", "type" : "thinboard_circle", "style" : ("attach",),
							"x" : 11, "y" : 12,
							"width" : 174, "height" : 81,
							"children" :
							(
								{
									"name":"InputSlot", "type":"image",
									"x": 8, "y":20,
									"image" : "d:/ymir work/ui/public/Parameter_Slot_00.sub",
									"children" :
									(
										{
											"name":"WonIcon", "type":"expanded_image", "style" : ("attach",),
											"x": 3, "y": 3,
											"x_scale" : .8, "y_scale" : .8,
											"image":"d:/ymir work/ui/game/windows/cheque_icon.sub",
										},
										{
											"name" : "Input", "type" : "editline_centered",
											"x" : 2, "y" : 3,
											"width" : 39, "height" : 18,
											"input_limit" : 3,
											"enable_codepage" : 0,
											"only_number" : 1,
											"r" : 1.0, "g" : 1.0, "b" : 1.0, "a" : 1.0,
										},
									),
								},
								{
									"name":"ArrowIcon", "type":"expanded_image", "style" : ("attach",),
									"x": 55, "y": 20,
									"x_scale" : 0.8, "y_scale" : .8,
									"image":"d:/ymir work/ui/game/windows/attach_metin_arrow.sub",
								},
								{
									"name":"ResultSlot", "type":"image",
									"x": 79, "y":20,
									"image" : "d:/ymir work/ui/public/parameter_slot_03.sub",
									"children" :
									(
										{
											"name":"GoldIcon", "type":"expanded_image", "style" : ("attach",),
											"x": 3, "y": 3,
											"x_scale" : .8, "y_scale" : .8,
											"image":"d:/ymir work/ui/game/windows/money_icon.sub",
										},
										{
											"name" : "Result", "type" : "text",
											"x" : 6, "y" : 3,
											"horizontal_align" : "right",
											"text_horizontal_align" : "right",
											"text" : "",
										},
									),
								},
								{
									"name" : "AcceptButton", "type" : "button",
									"horizontal_align" : "center",
									"x" : 0, "y" : 55,
									"default_image"	: "d:/ymir work/ui/public/acceptbutton00.sub",
									"over_image"	: "d:/ymir work/ui/public/acceptbutton01.sub",
									"down_image"	: "d:/ymir work/ui/public/acceptbutton02.sub",
								},
							),
						},
					),
				},
			),
		},
	),
}
