#!/usr/local/bin/python3
#### @martysama0134 dnd scripts ####

if __name__ == "__main__":
	with open("questlua_item.cpp") as f1:
		f1r = f1.read()

	f1r = f1r.replace('GetCurrentItem', 'GetCurrentDNDItem')
	f1r = f1r.replace('SetCurrentItem', 'SetCurrentDNDItem')
	f1r = f1r.replace('ClearCurrentItem', 'ClearCurrentDNDItem')
	f1r = f1r.replace('"item"', '"dnd"')
	f1r = f1r.replace('int item_', 'static int item_')
	f1r = f1r.replace('ALUA(item_', 'static ALUA(item_')
	f1r = f1r.replace('RegisterITEMFunctionTable', 'RegisterDNDFunctionTable')

	with open("questlua_dnd.cpp", "w") as f2:
		f1r= f1r.replace('namespace quest', '#include "quest.h"\n#ifdef ENABLE_QUEST_DND_EVENT\nnamespace quest') #UNDER_IFDEF_BEGIN
		f2.write(f1r)
		f2.write('#endif\n') #UNDER_IFDEF_END
