#ifndef __INC_METIN_II_GAME_SAFEBOX_H__
#define __INC_METIN_II_GAME_SAFEBOX_H__
#include <memory>

class CHARACTER;
class CItem;
class CGrid;

class CSafebox
{
	public:
		CSafebox(LPCHARACTER pkChrOwner, int iSize, DWORD dwGold);
		~CSafebox();

		bool		Add(DWORD dwPos, LPITEM pkItem);
		LPITEM		Get(DWORD dwPos) const;
		LPITEM		Remove(DWORD dwPos);
		void		ChangeSize(int iSize);

		bool		MoveItem(BYTE bCell, BYTE bDestCell, BYTE count);
		LPITEM		GetItem(BYTE bCell) const;

		void		Save() const;

		bool		IsEmpty(DWORD dwPos, BYTE bSize) const;
		bool		IsValidPosition(DWORD dwPos) const;

		void		SetWindowMode(BYTE bWindowMode);

	protected:
		void		__Destroy();

		LPCHARACTER	m_pkChrOwner;
		LPITEM		m_pkItems[SAFEBOX_MAX_NUM];
		std::unique_ptr<CGrid>		m_pkGrid; // @fixme191
		int		m_iSize;
		long		m_lGold;

		BYTE		m_bWindowMode;
};

#endif

