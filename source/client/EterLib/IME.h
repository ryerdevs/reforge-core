#pragma once

#include <imm.h>

#pragma comment(lib, "imm32.lib")

#include "DIMM.h"

class IIMEEventSink
{
public:
	virtual bool	OnWM_CHAR( WPARAM wParam, LPARAM lParam )	= 0;
	virtual void	OnUpdate()									= 0;

	virtual void	OnChangeCodePage()							= 0;

	virtual void	OnOpenCandidateList()						= 0;
	virtual void	OnCloseCandidateList()						= 0;

	virtual void	OnOpenReadingWnd()							= 0;
	virtual void	OnCloseReadingWnd()							= 0;
};

class CIME
{
public:
	enum
	{
		IMEREADING_MAXLEN = 128,
		IMESTR_MAXLEN = 1024,
		IMECANDIDATE_MAXLEN = 32768,
		MAX_CANDLIST = 10,
		MAX_CANDIDATE_LENGTH = 256
	};

public:
	CIME();
	virtual ~CIME();

	bool Initialize(HWND hWnd);
	void Uninitialize(void);

	static void Clear();

	void SetMax(int iMax);
	void SetUserMax(int iMax);
	void SetText(const char* c_szText, int len) const;
	int GetText(std::string & rstrText, bool addCodePage=false) const;
	const char* GetCodePageText() const;
	int GetCodePage() const;

	// Candidate List
	int  GetCandidateCount() const;
	int  GetCandidatePageCount() const;
	int  GetCandidate(DWORD index, std::string & rstrText) const;
	int  GetCandidateSelection() const;

	// Reading Information
	int GetReading(std::string & rstrText) const;
	int GetReadingError() const;

	void	SetInputMode(DWORD dwMode) const;
	DWORD	GetInputMode() const;

	bool	IsIMEEnabled() const;
	void	EnableIME(bool bEnable=true) const;
	void	DisableIME() const;

	void	EnableCaptureInput() const;
	void	DisableCaptureInput() const;
	bool	IsCaptureEnabled() const;

	void	SetNumberMode();
	void	SetStringMode();
	bool	__IsWritable(wchar_t key);
	void	AddExceptKey(wchar_t key);
	void	ClearExceptKey();

	void	PasteTextFromClipBoard() const;
	void	EnablePaste(bool bFlag);
	void	PasteString(const char * str) const;
	static void	FinalizeString(bool bSend = false);

	void	UseDefaultIME();

	static int GetCurPos();
	static int GetCompLen();
	static int GetULBegin();
	static int GetULEnd();

	static void CloseCandidateList();
	static void CloseReadingInformation();
	static void ChangeInputLanguage();
	static void ChangeInputLanguageWorker();

	LRESULT WMInputLanguage(HWND hWnd, UINT uiMsg, WPARAM wParam, LPARAM lParam) const;
	LRESULT WMStartComposition(HWND hWnd, UINT uiMsg, WPARAM wParam, LPARAM lParam) const;
	LRESULT WMComposition(HWND hWnd, UINT uiMsg, WPARAM wParam, LPARAM lParam);
	LRESULT WMEndComposition(HWND hWnd, UINT uiMsg, WPARAM wParam, LPARAM lParam) const;
	LRESULT WMNotify(HWND hWnd, UINT uiMsg, WPARAM wParam, LPARAM lParam) const;
	LRESULT WMChar(HWND hWnd, UINT uiMsg, WPARAM wParam, LPARAM lParam);

protected:
	void IncCurPos() const;
	void DecCurPos() const;
	void SetCurPos(int offset) const;
	void DelCurPos() const;

protected:
	static void CheckInputLocale();
	static void CheckToggleState();
	static void SetSupportLevel( DWORD dwImeLevel );

	void	InsertString(wchar_t* szString, int iSize) const;

	void	OnChar(wchar_t c);

	UINT	GetCodePageFromLang( LANGID langid ) const;
	void	ResultProcess(HIMC hImc) const;
	void	CompositionProcessBuilding(HIMC hImc);
	void	CompositionProcess(HIMC hImc);
	void	AttributeProcess(HIMC hImc) const;
	void	CandidateProcess(HIMC hImc) const;
	void	ReadingProcess(HIMC hImc) const;

	bool	IsMax(const wchar_t* wInput, int len) const;

	DWORD	GetImeId(UINT uIndex = 0) const;
	bool	GetReadingWindowOrientation() const;
	static	void	SetupImeApi();

	static INPUTCONTEXT*	(WINAPI * _ImmLockIMC)( HIMC );
	static BOOL		(WINAPI * _ImmUnlockIMC)( HIMC );
	static LPVOID	(WINAPI * _ImmLockIMCC)( HIMCC );
	static BOOL		(WINAPI * _ImmUnlockIMCC)( HIMCC );

	static UINT		(WINAPI * _GetReadingString)( HIMC, UINT, LPWSTR, PINT, BOOL*, PUINT );
	static BOOL		(WINAPI * _ShowReadingWindow)( HIMC, BOOL );

protected:
	HIMC			m_hOrgIMC;
	int				m_max;
	int				m_userMax;

	BOOL			m_bOnlyNumberMode;

	std::vector<wchar_t>	m_exceptKey;

	bool			m_bEnablePaste;
	bool			m_bUseDefaultIME;

public:
	static bool				ms_bInitialized;
	static bool				ms_bDisableIMECompletely;
	static bool				ms_bUILessMode;
	static bool				ms_bImeEnabled;
	static bool				ms_bCaptureInput;
	static bool				ms_bChineseIME;
	static bool				ms_bUseIMMCandidate;

	static HWND				ms_hWnd;
	static HKL				ms_hklCurrent;
	static char				ms_szKeyboardLayout[KL_NAMELENGTH+1];
	static OSVERSIONINFOA	ms_stOSVI;

	static HINSTANCE		ms_hImm32Dll;
	static HINSTANCE		ms_hCurrentImeDll;
	static DWORD			ms_dwImeState;

	static DWORD			ms_adwId[2];

	// IME Level
	static DWORD			ms_dwIMELevel;
	static DWORD			ms_dwIMELevelSaved;

	// Candidate List
	static bool				ms_bCandidateList;
	static DWORD			ms_dwCandidateCount;
	static bool				ms_bVerticalCandidate;
	static int				ms_iCandListIndexBase;
	static WCHAR			ms_wszCandidate[CIME::MAX_CANDLIST][MAX_CANDIDATE_LENGTH];
	static DWORD			ms_dwCandidateSelection;
	static DWORD			ms_dwCandidatePageSize;

	// Reading Information
	static bool				ms_bReadingInformation;
	static int				ms_iReadingError;
	static bool				ms_bHorizontalReading;
	static std::vector<wchar_t>	ms_wstrReading;

	// Indicator
	static 	wchar_t*		ms_wszCurrentIndicator;

	static IIMEEventSink*	ms_pEvent;

	wchar_t					m_wszComposition[IMESTR_MAXLEN];
	static wchar_t			m_wText[IMESTR_MAXLEN];

	static int				ms_compLen;
	static int				ms_curpos;
	static int				ms_lastpos;
	static int				ms_ulbegin;
	static int				ms_ulend;

	static UINT				ms_uOutputCodePage;
	static UINT				ms_uInputCodePage;
};

