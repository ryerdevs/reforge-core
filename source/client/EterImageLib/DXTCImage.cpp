// need the d3d.h for things in format of .dds file
#include "StdAfx.h"

#include <ddraw.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>

#include "../eterBase/MappedFile.h"
#include "../eterBase/Debug.h"

#include "DXTCImage.h"

struct DXTColBlock
{
	WORD col0;
	WORD col1;

	// no bit fields - use bytes
	BYTE row[4];
};

struct DXTAlphaBlockExplicit
{
	WORD row[4];
};

struct DXTAlphaBlock3BitLinear
{
	BYTE alpha0;
	BYTE alpha1;

	BYTE stuff[6];
};

// use cast to struct instead of RGBA_MAKE as struct is much
struct Color8888
{
	BYTE b;		// Last one is MSB, 1st is LSB.
	BYTE g;		// order of the output ARGB or BGRA, etc...
	BYTE r;		// change the order of names to change the
	BYTE a;
};

struct Color565
{
	unsigned nBlue  : 5;		// order of names changes
	unsigned nGreen : 6;		//  byte order of output to 32 bit
	unsigned nRed	: 5;
};

/////////////////////////////////////
// should be in ddraw.h
#ifndef MAKEFOURCC
#define MAKEFOURCC(ch0, ch1, ch2, ch3)                                      \
				((DWORD)(BYTE) (ch0       ) | ((DWORD)(BYTE) (ch1) <<  8) | \
				((DWORD)(BYTE) (ch2) << 16) | ((DWORD)(BYTE) (ch3) << 24))
#endif // defined(MAKEFOURCC)

CDXTCImage::CDXTCImage()
{
	Initialize();
}

CDXTCImage::~CDXTCImage()
{
}

void CDXTCImage::Initialize()
{
	m_nWidth = 0;
	m_nHeight = 0;

	for (int i = 0; i < MAX_MIPLEVELS; ++i)
		m_pbCompBufferByLevels[i] = nullptr;
}

void CDXTCImage::Clear()
{
	for (int i = 0; i < MAX_MIPLEVELS; ++i)
		m_bCompVector[i].clear();

	Initialize();
}

bool CDXTCImage::LoadFromFile(const char * filename)
{
	// only understands .dds files for now
	// return true if success
	const char * exts[] = { ".DDS" };
	const int next = 1;

	static char fileupper[MAX_PATH+1];

	strncpy(fileupper, filename, MAX_PATH);
	_strupr(fileupper);

	int i;
	bool knownformat = false;

	for (i = 0; i < next; ++i)
	{
		const char * found = strstr(fileupper, exts[0]);

		if (found != nullptr)
		{
			knownformat = true;
			break;
		}
	}

	if (knownformat == false)
	{
		Tracef("Unknown file format encountered! [%s]\n", filename);
		return(false);
	}

	CMappedFile mappedFile;
	LPCVOID pvMap;

	if (!mappedFile.Create(filename, &pvMap, 0, 0))
	{
		Tracef("Can't open file for reading! [%s]\n", filename);
		return false;
	}

	return LoadFromMemory((const BYTE*)pvMap, mappedFile.Size());
}

bool CDXTCImage::LoadHeaderFromMemory(const BYTE* c_pbMap, int iSize)
{
	if (iSize < sizeof(DWORD)) // @fixme042 size check
		return false;

	// Read magic number
	c_pbMap += sizeof(DWORD);
	iSize -= sizeof(DWORD); // @fixme042

	// Read the surface description
	if (iSize < sizeof(DDSURFACEDESC2)) // @fixme042 size check
		return false;

	DDSURFACEDESC2 ddsd{}; // read from dds file
	memcpy(&ddsd, c_pbMap, sizeof(DDSURFACEDESC2));
	c_pbMap += sizeof(DDSURFACEDESC2);
	iSize -= sizeof(DDSURFACEDESC2); // @fixme042

	// Does texture have mipmaps?
	m_bMipTexture = (ddsd.dwMipMapCount > 0) ? TRUE : FALSE;

	// Clear unwanted flags
	// Can't do this!!!  surface not re-created here
	//    ddsd.dwFlags &= (~DDSD_PITCH);
	//    ddsd.dwFlags &= (~DDSD_LINEARSIZE);

	// Is it DXTC ?
	// I sure hope pixelformat is valid!
	m_xddPixelFormat.dwFlags = ddsd.ddpfPixelFormat.dwFlags;
	m_xddPixelFormat.dwFourCC = ddsd.ddpfPixelFormat.dwFourCC;
	m_xddPixelFormat.dwSize = ddsd.ddpfPixelFormat.dwSize;
	m_xddPixelFormat.dwRGBBitCount = ddsd.ddpfPixelFormat.dwRGBBitCount;
	m_xddPixelFormat.dwRGBAlphaBitMask = ddsd.ddpfPixelFormat.dwRGBAlphaBitMask;
	m_xddPixelFormat.dwRBitMask = ddsd.ddpfPixelFormat.dwRBitMask;
	m_xddPixelFormat.dwGBitMask = ddsd.ddpfPixelFormat.dwGBitMask;
	m_xddPixelFormat.dwBBitMask = ddsd.ddpfPixelFormat.dwBBitMask;

	DecodePixelFormat(m_strFormat, &m_xddPixelFormat);

	if (m_CompFormat != PF_DXT1 &&
		m_CompFormat != PF_DXT3 &&
		m_CompFormat != PF_DXT5)
	{
		return false;
	}

	if (ddsd.dwMipMapCount > MAX_MIPLEVELS)
		ddsd.dwMipMapCount = MAX_MIPLEVELS;

	m_nWidth		= ddsd.dwWidth;
	m_nHeight		= ddsd.dwHeight;
	//!@#
	m_dwMipMapCount = max(1, ddsd.dwMipMapCount);
	m_dwFlags		= ddsd.dwFlags;

	if (ddsd.dwFlags & DDSD_PITCH)
	{
		m_lPitch = ddsd.lPitch;
		m_pbCompBufferByLevels[0] = c_pbMap;
	}
	else
	{
		m_lPitch = ddsd.dwLinearSize;

		if (ddsd.dwFlags & DDSD_MIPMAPCOUNT)
		{
			for (DWORD dwLinearSize = ddsd.dwLinearSize, i = 0; i < m_dwMipMapCount; ++i, dwLinearSize >>= 2)
			{
				m_pbCompBufferByLevels[i] = c_pbMap;
				c_pbMap += dwLinearSize;
			}
		}
		else
		{
			m_pbCompBufferByLevels[0] = c_pbMap;
		}
	}

	return true;
}

//////////////////////////////////////////////////////////////////////
bool CDXTCImage::LoadFromMemory(const BYTE* c_pbMap, int iSize)
{
	if (!LoadHeaderFromMemory(c_pbMap, iSize)) // @fixme042
		return false;

	if (m_dwFlags & DDSD_PITCH)
	{
		const DWORD dwBytesPerRow = m_nWidth * m_xddPixelFormat.dwRGBBitCount / 8;

		m_nCompSize = m_lPitch * m_nHeight;
		m_nCompLineSz = dwBytesPerRow;

		m_bCompVector[0].resize(m_nCompSize);
		BYTE * pDest = &m_bCompVector[0][0];

		c_pbMap = m_pbCompBufferByLevels[0];

		for (int yp = 0; yp < m_nHeight; ++yp)
		{
			memcpy(pDest, c_pbMap, dwBytesPerRow);
			pDest += m_lPitch;
			c_pbMap += m_lPitch;
		}
	}
	else
	{
		if (m_dwFlags & DDSD_MIPMAPCOUNT)
		{
			for (DWORD dwLinearSize = m_lPitch, i = 0; i < m_dwMipMapCount; ++i, dwLinearSize >>= 2)
			{
				m_bCompVector[i].resize(dwLinearSize);
				Copy(i, &m_bCompVector[i][0], dwLinearSize);
			}
		}
		else
		{
			m_bCompVector[0].resize(m_lPitch);
			Copy(0, &m_bCompVector[0][0], m_lPitch);
		}
	}

	// done reading file
	return true;
}

bool CDXTCImage::Copy(int miplevel, BYTE * pbDest, long lDestPitch) const
{
	if (!(m_dwFlags & DDSD_MIPMAPCOUNT))
		if (miplevel)
			return false;

	memcpy(pbDest, m_pbCompBufferByLevels[miplevel], m_lPitch >> (miplevel * 2));
	pbDest += lDestPitch;
	return true;
}

void CDXTCImage::Unextract(BYTE * pbDest, int /*iWidth*/, int /*iHeight*/, int iPitch) const
{
	if (!m_pbCompBufferByLevels[0])
		return;

	DXTColBlock * pBlock;
	const auto pPos = (BYTE *) &m_pbCompBufferByLevels[0][0];
	const int xblocks = m_nWidth / 4;
	const int yblocks = (m_nHeight / 4) * ((iPitch / m_nWidth) / 2);

	for (int y = 0; y < yblocks; ++y)
	{
		pBlock = (DXTColBlock*) (pPos + y * xblocks * 8);

		memcpy(pbDest, pBlock, xblocks * 8);
		pbDest += xblocks * 8;
	}
}

void CDXTCImage::Decompress(int miplevel, DWORD * pdwDest)
{
	switch (m_CompFormat)
	{
		case PF_DXT1:
			DecompressDXT1(miplevel, pdwDest);
			break;

		case PF_DXT3:
			DecompressDXT3(miplevel, pdwDest);
			break;

		case PF_DXT5:
			DecompressDXT5(miplevel, pdwDest);
			break;

		case PF_ARGB:
			DecompressARGB(miplevel, pdwDest);
			break;

		case PF_UNKNOWN:
			break;
	}
}

inline void GetColorBlockColors(DXTColBlock * pBlock, Color8888 * col_0, Color8888 * col_1,
								Color8888 * col_2, Color8888 * col_3,
								WORD & wrd)
{
	// There are 4 methods to use - see the Time_ functions.
	// 1st = shift = does normal approach per byte for color comps
	// 2nd = use freak variable bit field color565 for component extraction
	// 3rd = use super-freak DWORD adds BEFORE shifting the color components
	//  This lets you do only 1 add per color instead of 3 BYTE adds and
	//  might be faster
	// Call RunTimingSession() to run each of them & output result to txt file

	// freak variable bit structure method
	// normal math
	// This method is fastest
	Color565 * pCol;

	pCol = (Color565*) & (pBlock->col0);

	col_0->a = 0xff;
	col_0->r = pCol->nRed;
	col_0->r <<= 3;				// shift to full precision
	col_0->g = pCol->nGreen;
	col_0->g <<= 2;
	col_0->b = pCol->nBlue;
	col_0->b <<= 3;

	pCol = (Color565*) & (pBlock->col1);
	col_1->a = 0xff;
	col_1->r = pCol->nRed;
	col_1->r <<= 3;				// shift to full precision
	col_1->g = pCol->nGreen;
	col_1->g <<= 2;
	col_1->b = pCol->nBlue;
	col_1->b <<= 3;

	if (pBlock->col0 > pBlock->col1)
	{
		// Four-color block: derive the other two colors.
		// 00 = color_0, 01 = color_1, 10 = color_2, 11 = color_3
		// These two bit codes correspond to the 2-bit fields
		// stored in the 64-bit block.
		wrd = (WORD) (((WORD) col_0->r * 2 + (WORD) col_1->r) / 3);
		// no +1 for rounding
		// as bits have been shifted to 888
		col_2->r = (BYTE)wrd;

		wrd = (WORD) (((WORD) col_0->g * 2 + (WORD) col_1->g) / 3);
		col_2->g = (BYTE)wrd;

		wrd = (WORD) (((WORD) col_0->b * 2 + (WORD) col_1->b) / 3);
		col_2->b = (BYTE)wrd;
		col_2->a = 0xff;

		wrd = (WORD) (((WORD) col_0->r + (WORD) col_1->r * 2) / 3);
		col_3->r = (BYTE)wrd;

		wrd = (WORD) (((WORD) col_0->g + (WORD) col_1->g * 2) / 3);
		col_3->g = (BYTE)wrd;

		wrd = (WORD) (((WORD) col_0->b + (WORD) col_1->b * 2) / 3);
		col_3->b = (BYTE)wrd;
		col_3->a = 0xff;
	}
	else
	{
		// Three-color block: derive the other color.
		// 00 = color_0,  01 = color_1,  10 = color_2,
		// 11 = transparent.
		// These two bit codes correspond to the 2-bit fields
		// stored in the 64-bit block.

		// explicit for each component, unlike some refrasts...

		// Tracef("block has alpha\n");
		wrd = (WORD) (((WORD) col_0->r + (WORD) col_1->r) / 2);
		col_2->r = (BYTE)wrd;
		wrd = (WORD) (((WORD) col_0->g + (WORD) col_1->g) / 2);
		col_2->g = (BYTE)wrd;
		wrd = (WORD) (((WORD) col_0->b + (WORD) col_1->b) / 2);
		col_2->b = (BYTE)wrd;
		col_2->a = 0xff;

		col_3->r = 0x00;		// random color to indicate alpha
		col_3->g = 0x00;
		col_3->b = 0x00;
		col_3->a = 0x00;
	}
} // Get color block colors (...)

inline void DecodeColorBlock(DWORD * pImPos,
							 DXTColBlock * pColorBlock,
							 int width,
							 DWORD * col_0,
							 DWORD * col_1,
							 DWORD * col_2,
							 DWORD * col_3)
{
	// width is width of image in pixels
	DWORD bits;
	int y, n;

	// bit masks = 00000011, 00001100, 00110000, 11000000
	constexpr DWORD masks[] = { 3, 12, 3 << 4, 3 << 6 };
	constexpr int   shift[] = { 0, 2, 4, 6 };

	// r steps through lines in y
	for (y = 0; y < 4; ++y, pImPos += width - 4) // no width * 4 as DWORD ptr inc will * 4
	{
		// width * 4 bytes per pixel per line
		// each j dxtc row is 4 lines of pixels

		// pImPos = (DWORD*) ((DWORD) pBase + i * 16 + (y + j * 4) * m_nWidth * 4);

		// n steps through pixels
		for (n = 0; n < 4; ++n)
		{
			bits = pColorBlock->row[y] & masks[n];
			bits >>= shift[n];

			switch (bits)
			{
				case 0:
					*pImPos = *col_0;
					pImPos++; // increment to next DWORD
					break;

				case 1:
					*pImPos = *col_1;
					pImPos++;
					break;

				case 2:
					*pImPos = *col_2;
					pImPos++;
					break;

				case 3:
					*pImPos = *col_3;
					pImPos++;
					break;

				default:
					Tracef("Your logic is jacked! bits == 0x%x\n", bits);
					pImPos++;
					break;
			}
		}
	}
}

inline void DecodeAlphaExplicit(DWORD * pImPos, DXTAlphaBlockExplicit * pAlphaBlock,
								int width, DWORD alphazero)
{
	// alphazero is a bit mask that when & with the image color
	//  will zero the alpha bits, so if the image DWORDs  are
	//  ARGB then alphazero will be 0x00ffffff or if
	//  RGBA then alphazero will be 0xffffff00
	//  alphazero constructed automaticaly from field order of Color8888 structure

	// decodes to 32 bit format only
	int row, pix;

	WORD wrd;

	Color8888 col;
	col.r = col.g = col.b = 0;

	//Tracef("\n");
	for (row = 0; row < 4; row++, pImPos += width - 4)
	{
		// pImPow += pImPos += width-4 moves to next row down
		wrd = pAlphaBlock->row[row];

		// Tracef("0x%.8x\t\t", wrd);
		for (pix = 0; pix < 4; ++pix)
		{
			// zero the alpha bits of image pixel
			*pImPos &= alphazero;

			col.a = (BYTE) (wrd & 0x000f);		// get only low 4 bits
			//			col.a <<= 4;				// shift to full byte precision
			// NOTE:  with just a << 4 you'll never have alpha
			// of 0xff,  0xf0 is max so pure shift doesn't quite
			// cover full alpha range.
			// It's much cheaper than divide & scale though.
			// To correct for this, and get 0xff for max alpha,
			//  or the low bits back in after left shifting
			col.a = (BYTE) (col.a | (col.a << 4));	// This allows max 4 bit alpha to be 0xff alpha
			//  in final image, and is crude approach to full
			//  range scale

			*pImPos |= *((DWORD*)&col);	// or the bits into the prev. nulled alpha

			wrd >>= 4; // move next bits to lowest 4

			pImPos++; // move to next pixel in the row
		}
	}
}

static BYTE			gBits[4][4];
static WORD			gAlphas[8];
static Color8888	gACol[4][4];

inline void DecodeAlpha3BitLinear(DWORD * pImPos, DXTAlphaBlock3BitLinear * pAlphaBlock,
								  int width, DWORD alphazero)
{
	gAlphas[0] = pAlphaBlock->alpha0;
	gAlphas[1] = pAlphaBlock->alpha1;

	// 8-alpha or 6-alpha block?
	if (gAlphas[0] > gAlphas[1])
	{
		// 8-alpha block:  derive the other 6 alphas.
		// 000 = alpha_0, 001 = alpha_1, others are interpolated
		gAlphas[2] = (WORD) ((6 * gAlphas[0] +     gAlphas[1]) / 7);	// Bit code 010
		gAlphas[3] = (WORD) ((5 * gAlphas[0] + 2 * gAlphas[1]) / 7);	// Bit code 011
		gAlphas[4] = (WORD) ((4 * gAlphas[0] + 3 * gAlphas[1]) / 7);	// Bit code 100
		gAlphas[5] = (WORD) ((3 * gAlphas[0] + 4 * gAlphas[1]) / 7);	// Bit code 101
		gAlphas[6] = (WORD) ((2 * gAlphas[0] + 5 * gAlphas[1]) / 7);	// Bit code 110
		gAlphas[7] = (WORD) ((    gAlphas[0] + 6 * gAlphas[1]) / 7);	// Bit code 111
	}
	else
	{
		// 6-alpha block:  derive the other alphas.
		// 000 = alpha_0, 001 = alpha_1, others are interpolated
		gAlphas[2] = (WORD) ((4 * gAlphas[0] +     gAlphas[1]) / 5);	// Bit code 010
		gAlphas[3] = (WORD) ((3 * gAlphas[0] + 2 * gAlphas[1]) / 5);	// Bit code 011
		gAlphas[4] = (WORD) ((2 * gAlphas[0] + 3 * gAlphas[1]) / 5);	// Bit code 100
		gAlphas[5] = (WORD) ((   gAlphas[0] + 4 * gAlphas[1]) / 5);		// Bit code 101
		gAlphas[6] = 0;													// Bit code 110
		gAlphas[7] = 255;												// Bit code 111
	}

	// Decode 3-bit fields into array of 16 BYTES with same value

	// first two rows of 4 pixels each:
	// pRows = (Alpha3BitRows*) & (pAlphaBlock->stuff[0]);
	constexpr DWORD mask = 0x00000007;		// bits = 00 00 01 11
	DWORD bits = *((DWORD*) & (pAlphaBlock->stuff[0]));

	gBits[0][0] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[0][1] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[0][2] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[0][3] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[1][0] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[1][1] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[1][2] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[1][3] = (BYTE) (bits & mask);

	// now for last two rows:
	bits = *((DWORD*) & (pAlphaBlock->stuff[3]));		// last 3 bytes

	gBits[2][0] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[2][1] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[2][2] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[2][3] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[3][0] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[3][1] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[3][2] = (BYTE) (bits & mask);
	bits >>= 3;
	gBits[3][3] = (BYTE) (bits & mask);

	// decode the codes into alpha values
	int row, pix;

	for (row = 0; row < 4; ++row)
	{
		for (pix = 0; pix < 4; ++pix)
		{
			gACol[row][pix].a = (BYTE) gAlphas[gBits[row][pix]];

			assert(gACol[row][pix].r == 0);
			assert(gACol[row][pix].g == 0);
			assert(gACol[row][pix].b == 0);
		}
	}

	// Write out alpha values to the image bits
	for (row = 0; row < 4; ++row, pImPos += width - 4)
	{
		// pImPow += pImPos += width - 4 moves to next row down
		for (pix = 0; pix < 4; ++pix)
		{
			// zero the alpha bits of image pixel
			*pImPos &= alphazero;
			*pImPos |= *((DWORD*) &(gACol[row][pix]));	// or the bits into the prev. nulled alpha
			pImPos++;
		}
	}
}

void CDXTCImage::DecompressDXT1(int miplevel, DWORD * pdwDest)
{
	// This was hacked up pretty quick & slopily
	// decompresses to 32 bit format 0xARGB
	int xblocks, yblocks;
#ifdef DEBUG
	if ((ddsd.dwWidth % 4) != 0)
	{
		Tracef("****** warning width not div by 4!  %d\n", ddsd.dwWidth);
	}

	if ((ddsd.dwHeight % 4) != 0)
	{
		Tracef("****** warning Height not div by 4! %d\n", ddsd.dwHeight);
	}

	Tracef("end check\n");
#endif
	const UINT nWidth = m_nWidth >> miplevel;
	const UINT nHeight = m_nHeight >> miplevel;

	xblocks = nWidth / 4;
	yblocks = nHeight / 4;

	int		x, y;
	auto pBase	= (DWORD *) pdwDest;
	auto pPos	= (WORD *) &m_bCompVector[miplevel][0];	// pos in compressed data
	DWORD * pImPos;

	DXTColBlock	* pBlock;

	Color8888 col_0, col_1, col_2, col_3;
	WORD wrd;

	for (y = 0; y < yblocks; ++y)
	{
		// 8 bytes per block
		pBlock = (DXTColBlock *) ((DWORD) pPos + y * xblocks * 8);

		for (x = 0; x < xblocks; ++x, ++pBlock)
		{
			// inline func:
			GetColorBlockColors(pBlock, &col_0, &col_1, &col_2, &col_3, wrd);

			pImPos = (DWORD *) ((DWORD) pBase + x*16 + (y*4) * nWidth * 4);
			DecodeColorBlock(pImPos, pBlock, nWidth, (DWORD *)&col_0, (DWORD *)&col_1, (DWORD *)&col_2, (DWORD *)&col_3);
		}
	}
}

void CDXTCImage::DecompressDXT3(int miplevel, DWORD* pdwDest)
{
	int xblocks, yblocks;
#ifdef DEBUG
	if ((ddsd.dwWidth % 4) != 0)
	{
		Tracef("****** warning width not div by 4! %d\n", ddsd.dwWidth);
	}

	if ((ddsd.dwHeight % 4) != 0)
	{
		Tracef("****** warning Height not div by 4! %d\n", ddsd.dwHeight);
	}

	Tracef("end check\n");
#endif
	const UINT nWidth = m_nWidth >> miplevel;
	const UINT nHeight = m_nHeight >> miplevel;

	xblocks = nWidth / 4;
	yblocks = nHeight / 4;

	int		x, y;
	auto pBase	= (DWORD *) pdwDest;
	auto pPos	= (WORD *) &m_bCompVector[miplevel][0]; // pos in compressed data
	DWORD * pImPos;	// pos in decompressed data

	DXTColBlock	* pBlock;
	DXTAlphaBlockExplicit * pAlphaBlock;

	Color8888 col_0, col_1, col_2, col_3;
	WORD wrd;

	// fill alphazero with appropriate value to zero out alpha when
	//  alphazero is ANDed with the image color 32 bit DWORD:
	col_0.a = 0;
	col_0.r = col_0.g = col_0.b = 0xff;

	const DWORD alphazero = *((DWORD *) &col_0);

	for (y = 0; y < yblocks; ++y)
	{
		// 8 bytes per block
		// 1 block for alpha, 1 block for color
		pBlock = (DXTColBlock *) ((DWORD) (pPos + y * xblocks * 16));

		for (x = 0; x < xblocks; ++x, ++pBlock)
		{
			// inline
			// Get alpha block
			pAlphaBlock = (DXTAlphaBlockExplicit *) pBlock;

			// inline func:
			// Get color block & colors
			pBlock++;
			GetColorBlockColors(pBlock, &col_0, &col_1, &col_2, &col_3, wrd);

			// Decode the color block into the bitmap bits
			// inline func:
			pImPos = (DWORD *) ((DWORD) (pBase + x * 16 + (y * 4) * nWidth * 4));

			DecodeColorBlock(pImPos,
							 pBlock,
							 nWidth,
							 (DWORD *) &col_0, (DWORD *) &col_1, (DWORD *) &col_2, (DWORD *) &col_3);

			// Overwrite the previous alpha bits with the alpha block
			//  info
			// inline func:
			DecodeAlphaExplicit(pImPos, pAlphaBlock, nWidth, alphazero);
		}
	}
}

void CDXTCImage::DecompressDXT5(int level, DWORD * pdwDest)
{
	int xblocks, yblocks;
#ifdef DEBUG
	if ((ddsd.dwWidth % 4) != 0)
	{
		Tracef("****** warning width not div by 4! %d\n", ddsd.dwWidth);
	}

	if ((ddsd.dwHeight % 4) != 0)
	{
		Tracef("****** warning Height not div by 4! %d\n", ddsd.dwHeight);
	}

	Tracef("end check\n");
#endif
	const UINT nWidth = m_nWidth >> level;
	const UINT nHeight = m_nHeight >> level;

	xblocks = nWidth / 4;
	yblocks = nHeight / 4;

	int x, y;

	auto pBase = (DWORD *) pdwDest;
	WORD  * pPos = pPos = (WORD *) &m_bCompVector[level][0]; // pos in compressed data
	DWORD * pImPos;	// pos in decompressed data

	DXTColBlock	* pBlock;
	DXTAlphaBlock3BitLinear * pAlphaBlock;

	Color8888 col_0, col_1, col_2, col_3;
	WORD wrd;

	// fill alphazero with appropriate value to zero out alpha when
	// alphazero is ANDed with the image color 32 bit DWORD:
	col_0.a = 0;
	col_0.r = col_0.g = col_0.b = 0xff;
	const DWORD alphazero = *((DWORD *) &col_0);

	////////////////////////////////
	// Tracef("blocks: x: %d y: %d\n", xblocks, yblocks);
	for (y = 0; y < yblocks; ++y)
	{
		// 8 bytes per block
		// 1 block for alpha, 1 block for color
		pBlock = (DXTColBlock*) ((DWORD) (pPos + y * xblocks * 16));

		for (x = 0; x < xblocks; ++x, ++pBlock)
		{
			// inline
			// Get alpha block
			pAlphaBlock = (DXTAlphaBlock3BitLinear*) pBlock;

			// inline func:
			// Get color block & colors
			pBlock++;

			// Tracef("pBlock: 0x%.8x\n", pBlock);
			GetColorBlockColors(pBlock, &col_0, &col_1, &col_2, &col_3, wrd);

			// Decode the color block into the bitmap bits
			// inline func:
			pImPos = (DWORD *) ((DWORD) (pBase + x * 16 + (y * 4) * nWidth * 4));

			//DecodeColorBlock(pImPos, pBlock, nWidth, (DWORD *)&col_0, (DWORD *)&col_1, (DWORD *)&col_2, (DWORD *)&col_3);
			DecodeColorBlock(pImPos, pBlock, nWidth, (DWORD *)&col_0, (DWORD *)&col_1, (DWORD *)&col_2, (DWORD *)&col_3);

			// Overwrite the previous alpha bits with the alpha block
			//  info
			DecodeAlpha3BitLinear(pImPos, pAlphaBlock, nWidth, alphazero);
		}
	}
}	// dxt5

void CDXTCImage::DecompressARGB(int level, DWORD * pdwDest) const
{
	const UINT lPitch = m_lPitch >> (level * 2);
	memcpy(pdwDest, &m_bCompVector[level][0], lPitch);
}

//-----------------------------------------------------------------------------
// Name: GetNumberOfBits()
// Desc: Returns the number of bits set in a DWORD mask
//	from microsoft mssdk d3dim sample "Compress"
//-----------------------------------------------------------------------------
static WORD GetNumberOfBits(DWORD dwMask)
{
	WORD wBits;
    for (wBits = 0; dwMask; wBits++)
        dwMask = (dwMask & (dwMask - 1));

    return wBits;
}

//-----------------------------------------------------------------------------
// Name: PixelFormatToString()
// Desc: Creates a string describing a pixel format.
//	adapted from microsoft mssdk D3DIM Compress example
//  PixelFormatToString()
//-----------------------------------------------------------------------------
VOID CDXTCImage::DecodePixelFormat(CHAR* strPixelFormat, XDDPIXELFORMAT* pxddpf)
{
	switch (pxddpf->dwFourCC)
	{
		case 0:
			{
				// This dds texture isn't compressed so write out ARGB format
				const WORD a = GetNumberOfBits(pxddpf->dwRGBAlphaBitMask);
				const WORD r = GetNumberOfBits(pxddpf->dwRBitMask);
				const WORD g = GetNumberOfBits(pxddpf->dwGBitMask);
				const WORD b = GetNumberOfBits(pxddpf->dwBBitMask);

				_snprintf(strPixelFormat, 31, "ARGB-%d%d%d%d%s", a, r, g, b,
					pxddpf->dwBBitMask & DDPF_ALPHAPREMULT ? "-premul" : "");
				m_CompFormat = PF_ARGB;
			}
			break;

		case MAKEFOURCC('D','X','T','1'):
			strncpy(strPixelFormat, "DXT1", 31);
			m_CompFormat = PF_DXT1;
			break;

		case MAKEFOURCC('D','X','T','2'):
			strncpy(strPixelFormat, "DXT2", 31);
			m_CompFormat = PF_DXT2;
			break;

		case MAKEFOURCC('D','X','T','3'):
			strncpy(strPixelFormat, "DXT3", 31);
			m_CompFormat = PF_DXT3;
			break;

		case MAKEFOURCC('D','X','T','4'):
			strncpy(strPixelFormat, "DXT4", 31);
			m_CompFormat = PF_DXT4;
			break;

		case MAKEFOURCC('D','X','T','5'):
			strncpy(strPixelFormat, "DXT5", 31);
			m_CompFormat = PF_DXT5;
			break;

		default:
			strcpy(strPixelFormat, "Format Unknown");
			m_CompFormat = PF_UNKNOWN;
			break;
	}
}

