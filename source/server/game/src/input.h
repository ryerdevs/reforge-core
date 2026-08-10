#ifndef __INC_METIN_II_GAME_INPUT_PROCESSOR__
#define __INC_METIN_II_GAME_INPUT_PROCESSOR__

#include "packet_info.h"

enum
{
	INPROC_CLOSE,
	INPROC_HANDSHAKE,
	INPROC_LOGIN,
	INPROC_MAIN,
	INPROC_DEAD,
	INPROC_DB,
	INPROC_P2P,
	INPROC_AUTH,
};

void LoginFailure(LPDESC d, const char * c_pszStatus);
extern void SendShout(const char * szText, BYTE bEmpire);

#ifdef ENABLE_CHANNEL_STATUS_CACHE
#include <vector>
extern std::vector<char> cachedChannelStatus;
#endif

class CInputProcessor
{
	public:
		CInputProcessor();
		virtual ~CInputProcessor() {};

		virtual bool Process(LPDESC d, const void * c_pvOrig, int iBytes, int & r_iBytesProceed);
		virtual BYTE GetType() = 0;

		void BindPacketInfo(CPacketInfo * pPacketInfo);
		void Pong(LPDESC d) const;
		void Handshake(LPDESC d, const char * c_pData) const;
		void Version(LPCHARACTER ch, const char* c_pData) const;

	protected:
		virtual int	Analyze(LPDESC d, BYTE bHeader, const char * c_pData) = 0;

		CPacketInfo * m_pPacketInfo;
		int	m_iBufferLeft;

		CPacketInfoCG 	m_packetInfoCG;
};

class CInputClose : public CInputProcessor
{
	public:
		virtual BYTE	GetType() { return INPROC_CLOSE; }

	protected:
		virtual int	Analyze(LPDESC d, BYTE bHeader, const char * c_pData) { return m_iBufferLeft; }
};

class CInputHandshake : public CInputProcessor
{
	public:
		CInputHandshake();
		virtual ~CInputHandshake();

		virtual BYTE	GetType() { return INPROC_HANDSHAKE; }

	protected:
		virtual int	Analyze(LPDESC d, BYTE bHeader, const char * c_pData);

	protected:
		void		GuildMarkLogin(LPDESC d, const char* c_pData);

		CPacketInfo *	m_pMainPacketInfo;
};

class CInputLogin : public CInputProcessor
{
	public:
		virtual BYTE	GetType() { return INPROC_LOGIN; }

	protected:
		virtual int	Analyze(LPDESC d, BYTE bHeader, const char * c_pData);

	protected:
		void		Login(LPDESC d, const char * data) const;
		void		LoginByKey(LPDESC d, const char * data) const;

		void		CharacterSelect(LPDESC d, const char * data) const;
		void		CharacterCreate(LPDESC d, const char * data) const;
		void		CharacterDelete(LPDESC d, const char * data) const;
		void		Entergame(LPDESC d, const char * data) const;
		void		Empire(LPDESC d, const char * c_pData) const;
		void		GuildMarkCRCList(LPDESC d, const char* c_pData) const;
		// MARK_BUG_FIX
		void		GuildMarkIDXList(LPDESC d, const char* c_pData) const;
		// END_OF_MARK_BUG_FIX
		void		GuildMarkUpload(LPDESC d, const char* c_pData) const;
		int			GuildSymbolUpload(LPDESC d, const char* c_pData, size_t uiBytes) const;
		void		GuildSymbolCRC(LPDESC d, const char* c_pData) const;
		void		ChangeName(LPDESC d, const char * data) const;
};

class CInputMain : public CInputProcessor
{
	public:
		virtual BYTE	GetType() { return INPROC_MAIN; }

	protected:
		virtual int	Analyze(LPDESC d, BYTE bHeader, const char * c_pData);

	protected:
		void		Attack(LPCHARACTER ch, const BYTE header, const char* data) const;

		int			Whisper(LPCHARACTER ch, const char * data, size_t uiBytes) const;
		int			Chat(LPCHARACTER ch, const char * data, size_t uiBytes) const;
		void		ItemUse(LPCHARACTER ch, const char * data) const;
		void		ItemDrop(LPCHARACTER ch, const char * data) const;
		void		ItemDrop2(LPCHARACTER ch, const char * data) const;
		void		ItemMove(LPCHARACTER ch, const char * data) const;
		void		ItemPickup(LPCHARACTER ch, const char * data) const;
		void		ItemToItem(LPCHARACTER ch, const char * pcData) const;
		void		QuickslotAdd(LPCHARACTER ch, const char * data) const;
		void		QuickslotDelete(LPCHARACTER ch, const char * data) const;
		void		QuickslotSwap(LPCHARACTER ch, const char * data) const;
		int			Shop(LPCHARACTER ch, const char * data, size_t uiBytes) const;
		void		OnClick(LPCHARACTER ch, const char * data) const;
		void		Exchange(LPCHARACTER ch, const char * data) const;
		void		Position(LPCHARACTER ch, const char * data) const;
		void		Move(LPCHARACTER ch, const char * data) const;
		int			SyncPosition(LPCHARACTER ch, const char * data, size_t uiBytes) const;
		void		FlyTarget(LPCHARACTER ch, const char * pcData, BYTE bHeader) const;
		void		UseSkill(LPCHARACTER ch, const char * pcData) const;

		void		ScriptAnswer(LPCHARACTER ch, const void * pvData) const;
		void		ScriptButton(LPCHARACTER ch, const void * pvData) const;
		void		ScriptSelectItem(LPCHARACTER ch, const void * pvData) const;

		void		QuestInputString(LPCHARACTER ch, const void * pvData) const;
		void		QuestConfirm(LPCHARACTER ch, const void* pvData) const;
		void		Target(LPCHARACTER ch, const char * pcData) const;
		void		Warp(LPCHARACTER ch, const char * pcData) const;
		void		SafeboxCheckin(LPCHARACTER ch, const char * c_pData) const;
		void		SafeboxCheckout(LPCHARACTER ch, const char * c_pData, bool bMall) const;
		void		SafeboxItemMove(LPCHARACTER ch, const char * data) const;
		int			Messenger(LPCHARACTER ch, const char* c_pData, size_t uiBytes) const;

		void 		PartyInvite(LPCHARACTER ch, const char * c_pData) const;
		void 		PartyInviteAnswer(LPCHARACTER ch, const char * c_pData) const;
		void		PartyRemove(LPCHARACTER ch, const char * c_pData) const;
		void		PartySetState(LPCHARACTER ch, const char * c_pData) const;
		void		PartyUseSkill(LPCHARACTER ch, const char * c_pData) const;
		void		PartyParameter(LPCHARACTER ch, const char * c_pData) const;

		int			Guild(LPCHARACTER ch, const char * data, size_t uiBytes) const;
		void		AnswerMakeGuild(LPCHARACTER ch, const char* c_pData) const;

		void		Fishing(LPCHARACTER ch, const char* c_pData) const;
		void		ItemGive(LPCHARACTER ch, const char* c_pData) const;
		void		Hack(LPCHARACTER ch, const char * c_pData) const;
		int			MyShop(LPCHARACTER ch, const char * c_pData, size_t uiBytes) const;

		void		Refine(LPCHARACTER ch, const char* c_pData) const;
#ifdef ENABLE_ACCE_COSTUME_SYSTEM
		void		Acce(LPCHARACTER pkChar, const char* c_pData) const;
#endif
};

class CInputDead : public CInputMain
{
	public:
		virtual BYTE	GetType() { return INPROC_DEAD; }

	protected:
		virtual int	Analyze(LPDESC d, BYTE bHeader, const char * c_pData);
};

class CInputDB : public CInputProcessor
{
public:
	virtual bool Process(LPDESC d, const void * c_pvOrig, int iBytes, int & r_iBytesProceed);
	virtual BYTE GetType() { return INPROC_DB; }

protected:
	virtual int	Analyze(LPDESC d, BYTE bHeader, const char * c_pData);

protected:
	void		MapLocations(const char * c_pData) const;
	void		LoginSuccess(DWORD dwHandle, const char *data) const;
	void		PlayerCreateFailure(LPDESC d, BYTE bType) const;
	void		PlayerDeleteSuccess(LPDESC d, const char * data) const;
	void		PlayerDeleteFail(LPDESC d) const;
	void		PlayerLoad(LPDESC d, const char* data) const;
	void		PlayerCreateSuccess(LPDESC d, const char * data) const;
	void		Boot(const char* data) const;
	void		QuestLoad(LPDESC d, const char * c_pData) const;
	void		SafeboxLoad(LPDESC d, const char * c_pData) const;
	void		SafeboxChangeSize(LPDESC d, const char * c_pData) const;
	void		SafeboxWrongPassword(LPDESC d) const;
	void		SafeboxChangePasswordAnswer(LPDESC d, const char* c_pData) const;
	void		MallLoad(LPDESC d, const char * c_pData) const;
	void		EmpireSelect(LPDESC d, const char * c_pData) const;
	void		P2P(const char * c_pData) const;
	void		ItemLoad(LPDESC d, const char * c_pData) const;
	void		AffectLoad(LPDESC d, const char * c_pData) const;

	void		GuildLoad(const char * c_pData) const;
	void		GuildSkillUpdate(const char* c_pData) const;
	void		GuildSkillRecharge() const;
	void		GuildExpUpdate(const char* c_pData) const;
	void		GuildAddMember(const char* c_pData) const;
	void		GuildRemoveMember(const char* c_pData) const;
	void		GuildChangeGrade(const char* c_pData) const;
	void		GuildChangeMemberData(const char* c_pData) const;
	void		GuildDisband(const char* c_pData) const;
	void		GuildLadder(const char* c_pData) const;
	void		GuildWar(const char* c_pData) const;
	void		GuildWarScore(const char* c_pData) const;
	void		GuildSkillUsableChange(const char* c_pData) const;
	void		GuildMoneyChange(const char* c_pData) const;
	void		GuildWithdrawMoney(const char* c_pData) const;
	void		GuildWarReserveAdd(TGuildWarReserve * p) const;
	void		GuildWarReserveUpdate(TGuildWarReserve * p);
	void		GuildWarReserveDelete(DWORD dwID) const;
	void		GuildWarBet(TPacketGDGuildWarBet * p) const;
	void		GuildChangeMaster(TPacketChangeGuildMaster* p) const;

	void		LoginAlready(LPDESC d, const char * c_pData) const;

	void		PartyCreate(const char* c_pData) const;
	void		PartyDelete(const char* c_pData) const;
	void		PartyAdd(const char* c_pData) const;
	void		PartyRemove(const char* c_pData) const;
	void		PartyStateChange(const char* c_pData) const;
	void		PartySetMemberLevel(const char* c_pData) const;

	void		Time(const char * c_pData) const;

	void		ReloadProto(const char * c_pData);
	void		ChangeName(LPDESC d, const char * data) const;

	void		AuthLogin(LPDESC d, const char * c_pData) const;
	void		ItemAward(const char * c_pData);

	void		ChangeEmpirePriv(const char* c_pData) const;
	void		ChangeGuildPriv(const char* c_pData) const;
	void		ChangeCharacterPriv(const char* c_pData) const;

	void		MoneyLog(const char* c_pData) const;

	void		SetEventFlag(const char* c_pData) const;

	void		CreateObject(const char * c_pData) const;
	void		DeleteObject(const char * c_pData) const;
	void		UpdateLand(const char * c_pData) const;

	void		Notice(const char * c_pData) const;

	void		MarriageAdd(TPacketMarriageAdd * p) const;
	void		MarriageUpdate(TPacketMarriageUpdate * p) const;
	void		MarriageRemove(TPacketMarriageRemove * p) const;

	void		WeddingRequest(TPacketWeddingRequest* p) const;
	void		WeddingReady(TPacketWeddingReady* p) const;
	void		WeddingStart(TPacketWeddingStart* p) const;
	void		WeddingEnd(TPacketWeddingEnd* p) const;

	void		TakeMonarchMoney(LPDESC d, const char * data ) const;
	void		AddMonarchMoney(LPDESC d, const char * data ) const;
	void		DecMonarchMoney(LPDESC d, const char * data ) const;
	void		SetMonarch( LPDESC d, const char * data );

	void		ChangeMonarchLord(TPacketChangeMonarchLordACK* data) const;
	void		UpdateMonarchInfo(TMonarchInfo* data) const;

	// MYSHOP_PRICE_LIST

	void		MyshopPricelistRes( LPDESC d, const TPacketMyshopPricelistHeader* p ) const;
	// END_OF_MYSHOP_PRICE_LIST

	//RELOAD_ADMIN
	void ReloadAdmin( const char * c_pData ) const;
	//END_RELOAD_ADMIN

	void		DetailLog(const TPacketNeedLoginLogInfo* info) const;

	void		ItemAwardInformer(TPacketItemAwardInfromer* data) const;

	void		RespondChannelStatus(LPDESC desc, const char* pcData) const;

	protected:
		DWORD		m_dwHandle;
};

class CInputP2P : public CInputProcessor
{
	public:
		CInputP2P();
		virtual BYTE	GetType() { return INPROC_P2P; }

	protected:
		virtual int	Analyze(LPDESC d, BYTE bHeader, const char * c_pData);

	public:
		void		Setup(LPDESC d, const char * c_pData) const;
		void		Login(LPDESC d, const char * c_pData) const;
		void		Logout(LPDESC d, const char * c_pData) const;
		int			Relay(LPDESC d, const char * c_pData, size_t uiBytes) const;
#ifdef ENABLE_FULL_NOTICE
		int			Notice(LPDESC d, const char * c_pData, size_t uiBytes, bool bBigFont=false) const;
#else
		int			Notice(LPDESC d, const char * c_pData, size_t uiBytes);
#endif
		int			MonarchNotice(LPDESC d, const char * c_pData, size_t uiBytes) const;
		int			MonarchTransfer(LPDESC d, const char * c_pData) const;
		int			Guild(LPDESC d, const char* c_pData, size_t uiBytes) const;
		void		Shout(const char * c_pData) const;
		void		Disconnect(const char * c_pData) const;
		void		MessengerAdd(const char * c_pData) const;
		void		MessengerRemove(const char * c_pData) const;
		void		FindPosition(LPDESC d, const char* c_pData) const;
		void		WarpCharacter(const char* c_pData) const;
		void		GuildWarZoneMapIndex(const char* c_pData) const;
		void		Transfer(const char * c_pData) const;
		void		XmasWarpSanta(const char * c_pData) const;
		void		XmasWarpSantaReply(const char * c_pData) const;
		void		LoginPing(LPDESC d, const char * c_pData) const;
		void		BlockChat(const char * c_pData) const;
		void		IamAwake(LPDESC d, const char * c_pData) const;

	protected:
		CPacketInfoGG 	m_packetInfoGG;
};

class CInputAuth : public CInputProcessor
{
	public:
		CInputAuth();
		virtual BYTE GetType() { return INPROC_AUTH; }

	protected:
		virtual int	Analyze(LPDESC d, BYTE bHeader, const char * c_pData);

	public:
		void		Login(LPDESC d, const char * c_pData) const;
};

#endif /* __INC_METIN_II_GAME_INPUT_PROCESSOR__ */

