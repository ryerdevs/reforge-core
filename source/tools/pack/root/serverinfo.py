
import app
import localeInfo
from constInfo import TextColor
app.ServerName = None

ENABLE_RANDOM_AUTH_NO_LIB = True
if ENABLE_RANDOM_AUTH_NO_LIB:
	from time import time as time_time
	def get_random_number(upper_limit):
		_timestamp = time_time()
		_timestamp = int(_timestamp*1000000)
		return _timestamp % upper_limit

	def get_item_from_list(_list):
		choice = get_random_number(len(_list))
		assert choice < len(_list), "Index should be less than length of list"
		return _list[choice]
else:
	from random import choice as random_choice
	def get_item_from_list(_list):
		return random_choice(_list)

SRV1 = {
	"name":TextColor("ServerName", "ffd500"), #GOLD
	"host":"127.0.0.1",
	"srv1-auth1":30001,
	"ch1":30003,
	"ch2":30007,
	"ch3":30011,
	"ch4":30015,

	"authlist": [ 30001, ],
}

STATE_NONE = TextColor(localeInfo.CHANNEL_STATUS_OFFLINE, "FF0000") #RED

STATE_DICT = {
	0: TextColor(localeInfo.CHANNEL_STATUS_OFFLINE, "FF0000"), #RED
	1: TextColor(localeInfo.CHANNEL_STATUS_RECOMMENDED, "00ff00"), #GREEN
	2: TextColor(localeInfo.CHANNEL_STATUS_BUSY, "ffff00"), #YELLOW
	3: TextColor(localeInfo.CHANNEL_STATUS_FULL, "ff8a08") #ORANGE
}

SERVER1_CHANNEL_DICT = {
	0: {"key":10, "name":TextColor("CH-1", "FFffFF"), "ip":SRV1["host"], "tcp_port":SRV1["ch1"], "udp_port":SRV1["ch1"], "state":STATE_DICT[1],},
	1: {"key":11, "name":TextColor("CH-2", "FFffFF"), "ip":SRV1["host"], "tcp_port":SRV1["ch2"], "udp_port":SRV1["ch2"], "state":STATE_DICT[1],},
	2: {"key":12, "name":TextColor("CH-3", "FFffFF"), "ip":SRV1["host"], "tcp_port":SRV1["ch3"], "udp_port":SRV1["ch3"], "state":STATE_DICT[1],},
	3: {"key":13, "name":TextColor("CH-4", "FFffFF"), "ip":SRV1["host"], "tcp_port":SRV1["ch4"], "udp_port":SRV1["ch4"], "state":STATE_DICT[1],},

}

REGION_NAME_DICT = {
	0: SRV1["name"],
}

REGION_AUTH_SERVER_DICT = {
	0: {
		1: {"ip": SRV1["host"], "port": get_item_from_list(SRV1["authlist"]),},
		2: {"ip": SRV1["host"], "port": get_item_from_list(SRV1["authlist"]),},
		3: {"ip": SRV1["host"], "port": get_item_from_list(SRV1["authlist"]),},
		4: {"ip": SRV1["host"], "port": get_item_from_list(SRV1["authlist"]),},

	}
}

REGION_DICT = {
	0: {
		1: {"name": SRV1["name"], "channel": SERVER1_CHANNEL_DICT,},
	},
}

MARKADDR_DICT = {
	10: {"ip": SRV1["host"], "tcp_port": SRV1["ch1"], "mark": "10.tga", "symbol_path": "10",},
}

TESTADDR = {"ip": SRV1["host"], "tcp_port": SRV1["ch1"], "udp_port": SRV1["ch1"],}
