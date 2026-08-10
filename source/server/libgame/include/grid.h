#ifndef __INC_METIN_II_GRID_H__
#define __INC_METIN_II_GRID_H__

class CGrid
{
    public:
	CGrid(int w, int h);
	CGrid(CGrid * pkGrid, int w, int h);
	~CGrid();

	void		Clear() const;
	int		FindBlank(int w, int h) const;
	bool		IsEmpty(int iPos, int w, int h) const;
	bool		Put(int iPos, int w, int h) const;
	void		Get(int iPos, int w, int h) const;
	void		Print();
	unsigned int	GetSize() const;

    protected:
	int	m_iWidth;
	int	m_iHeight;

	char *	m_pGrid;
};

#endif

