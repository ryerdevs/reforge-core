#include "stdafx.h"
#include "constants.h"
#include "char.h"
#include "desc.h"
#include "desc_manager.h"
#include "packet.h"
#include "item.h"

/////////////////////////////////////////////////////////////////////////////
// QUICKSLOT HANDLING
/////////////////////////////////////////////////////////////////////////////
void CHARACTER::SyncQuickslot(BYTE bType, BYTE bOldPos, BYTE bNewPos) const
{
	if (!m_PlayerSlots)
		return;

	if (bOldPos == bNewPos)
		return;

	for (int i = 0; i < QUICKSLOT_MAX_NUM; ++i)
	{
		if (m_PlayerSlots->pQuickslot[i].type == bType && m_PlayerSlots->pQuickslot[i].pos == bOldPos)
		{
			if (bNewPos == 255)
				DelQuickslot(i);
			else
			{
				TQuickslot slot;

				slot.type = bType;
				slot.pos = bNewPos;

				SetQuickslot(i, slot);
			}
		}
	}
}

bool CHARACTER::GetQuickslot(BYTE pos, TQuickslot ** ppSlot) const
{
	if (!m_PlayerSlots)
		return false;

	if (pos >= QUICKSLOT_MAX_NUM)
		return false;

	*ppSlot = &m_PlayerSlots->pQuickslot[pos];
	return true;
}

bool CHARACTER::SetQuickslot(BYTE pos, TQuickslot & rSlot) const
{
	if (!m_PlayerSlots)
		return false;

	if (pos >= QUICKSLOT_MAX_NUM)
		return false;

	if (rSlot.type >= QUICKSLOT_TYPE_MAX_NUM)
		return false;

	struct packet_quickslot_add pack_quickslot_add;
	for (int i = 0; i < QUICKSLOT_MAX_NUM; ++i)
	{
		if (rSlot.type == 0)
			continue;
		else if (m_PlayerSlots->pQuickslot[i].type == rSlot.type && m_PlayerSlots->pQuickslot[i].pos == rSlot.pos)
			DelQuickslot(i);
	}

	const TItemPos srcCell(INVENTORY, rSlot.pos);

	switch (rSlot.type)
	{
		case QUICKSLOT_TYPE_ITEM:
			if (false == srcCell.IsDefaultInventoryPosition() && false == srcCell.IsBeltInventoryPosition())
				return false;

			break;

		case QUICKSLOT_TYPE_SKILL:
			if ((int) rSlot.pos >= SKILL_MAX_NUM)
				return false;

			break;

		case QUICKSLOT_TYPE_COMMAND:
			break;

		default:
			return false;
	}

	m_PlayerSlots->pQuickslot[pos] = rSlot;

	if (GetDesc())
	{
		pack_quickslot_add.header	= HEADER_GC_QUICKSLOT_ADD;
		pack_quickslot_add.pos		= pos;
		pack_quickslot_add.slot		= m_PlayerSlots->pQuickslot[pos];

		GetDesc()->Packet(&pack_quickslot_add, sizeof(pack_quickslot_add));
	}

	return true;
}

bool CHARACTER::DelQuickslot(BYTE pos) const
{
	if (!m_PlayerSlots)
		return false;

	if (pos >= QUICKSLOT_MAX_NUM)
		return false;

	memset(&m_PlayerSlots->pQuickslot[pos], 0, sizeof(TQuickslot));

	struct packet_quickslot_del pack_quickslot_del;
	pack_quickslot_del.header	= HEADER_GC_QUICKSLOT_DEL;
	pack_quickslot_del.pos	= pos;

	GetDesc()->Packet(&pack_quickslot_del, sizeof(pack_quickslot_del));
	return true;
}

bool CHARACTER::SwapQuickslot(BYTE a, BYTE b) const
{
	if (!m_PlayerSlots)
		return false;

	if (a >= QUICKSLOT_MAX_NUM || b >= QUICKSLOT_MAX_NUM)
		return false;

	TQuickslot quickslot;
	quickslot = m_PlayerSlots->pQuickslot[a];

	m_PlayerSlots->pQuickslot[a] = m_PlayerSlots->pQuickslot[b];
	m_PlayerSlots->pQuickslot[b] = quickslot;

	struct packet_quickslot_swap pack_quickslot_swap;
	pack_quickslot_swap.header	= HEADER_GC_QUICKSLOT_SWAP;
	pack_quickslot_swap.pos	= a;
	pack_quickslot_swap.pos_to	= b;

	GetDesc()->Packet(&pack_quickslot_swap, sizeof(pack_quickslot_swap));
	return true;
}

void CHARACTER::ChainQuickslotItem(LPITEM pItem, BYTE bType, BYTE bOldPos) const
{
	if (!m_PlayerSlots)
		return;

	if (pItem->IsDragonSoul())
		return;

	for ( int i=0; i < QUICKSLOT_MAX_NUM; ++i )
	{
		if ( m_PlayerSlots->pQuickslot[i].type == bType && m_PlayerSlots->pQuickslot[i].pos == bOldPos )
		{
			TQuickslot slot;
			slot.type = bType;
			slot.pos = pItem->GetCell();

			SetQuickslot(i, slot);

			break;
		}
	}
}

