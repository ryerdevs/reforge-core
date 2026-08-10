# README #

This README would normally document whatever steps are necessary to get your application up and running.

### What is this repository for? ###

* Quick summary
* Version
* [Learn Markdown](https://bitbucket.org/tutorials/markdowndemo)

### How do I get set up? ###

* Summary of set up
* Configuration
* Dependencies
* Database configuration
* How to run tests
* Deployment instructions

### Contribution guidelines ###

* Writing tests
* Code review
@warme001: small changes

#@Srcs\Scripts\client\pack\root
@fixme001: on intrologin.py; timeOutMsg and timeOutOk undefined, and some error will occur after login timeout
@fixme002: on uishop.py; shop_ex 3 buttons will click button2 instead of button1 at loading
@fixme003: on game.py; DS items highlighting even before interface was loading fix
@fixme004: on utils.py; uiscript load will print the error in syserr.txt if failed now
@fixme005: on uiinventory.py; if the mouse-selected yang is over an item with sockets in the inventory, the next tooltip will be blank
@fixme007: on uiprivateshopbuilder.py; the shop signs weren't cleared at re-warp so mobs from other game cores could have shop names on them
@fixme008: on uitooltip.py; leadership skill (guide)'s tooltip fix
@fixme009: on uisafebox.py; "/safebox_close" and "/mall_close" will be flooded like crazy after going out of the limit range causing a logout
@fixme011: on uidragonsoul.py, uiinventory.py; moving equipped ds slots to others would automatically trigger the unequipment (UseItem); it also couldn't reselect the same slot to deattach the mouse
@fixme012: on uitooltip.py; items with no remain time were showing "remain time 0" in shops
@fixme013: on uichat.py; the last shout time was wrongly calculated at rewarp
@fixme014: on uiminimap.py; the minimap wasn't toggling the atlas window properly
@fixme015: on prototype.py; mouseModule was static and not properly cleared from RunApp()
@fixme016: on interfacemodule.py; uiQuest.QuestCurtain wasn't cleared properly
@fixme017: on uitooltip.py; AutoAppendTextLine wasn't calculating the new width position properly
@fixme018: on ui.py; EditLine.OnPressEscapeKey was returning True even if unfocused
@fixme019: on intrologin.py; unwanted focus on the inputs when selecting the server board
@fixme020: on game.py; missing formatted dots when picking up yang
@fixme021: on intrologin.py; when disconnected, the ServerName field remains noname

* Other guidelines

### How to sort npclist.txt ###
$ sort -k1 -n npclist.txt > npclist_sorted.txt

### Who do I talk to? ###

* Repo owner or admin
martysama0134
