#ifndef __INC_METIN_II_371GNFBQOCJ_LZO_H__
#define __INC_METIN_II_371GNFBQOCJ_LZO_H__

#include <windows.h>
#include <lzo/lzo1x.h>
#include "../../../Client/EterBase/Singleton.h"
#include <vector>

class CLZObject
{
	public:
		#pragma pack(4)
			typedef struct SHeader
			{
				DWORD	dwFourCC;
				DWORD	dwEncryptSize;
				DWORD	dwCompressedSize;
				DWORD	dwRealSize;
			} THeader;
		#pragma pack()

		CLZObject();
		~CLZObject();

		void			Clear();

		void			BeginCompress(const void * pvIn, UINT uiInLen);
		void			BeginCompressInBuffer(const void * pvIn, UINT uiInLen, void * pvOut);
		bool			Compress();

		bool			BeginDecompress(const void * pvIn);
		bool			Decompress(DWORD * pdwKey = nullptr) const;

		bool			Encrypt(DWORD * pdwKey) const;
		bool			__Decrypt(DWORD * key, BYTE* data) const;

		const THeader &	GetHeader() const { return *m_pHeader; }
		BYTE *			GetBuffer() const { return m_pbBuffer; }
		DWORD			GetSize() const;
		void			AllocBuffer(DWORD dwSize);
		DWORD			GetBufferSize() const { return m_dwBufferSize; }
		//void			CopyBuffer(const char* pbSrc, DWORD dwSrcSize);

	private:
		void			Initialize();

		BYTE *			m_pbBuffer;
		DWORD			m_dwBufferSize;

		THeader	*		m_pHeader;
		const BYTE *	m_pbIn;
		bool			m_bCompressed;

		bool			m_bInBuffer;

	public:
		static DWORD	ms_dwFourCC;
};

class CLZO : public CSingleton<CLZO>
{
	public:
		CLZO();
		virtual ~CLZO();

		bool	CompressMemory(CLZObject & rObj, const void * pIn, UINT uiInLen) const;
		bool	CompressEncryptedMemory(CLZObject & rObj, const void * pIn, UINT uiInLen, DWORD * pdwKey) const;
		bool	Decompress(CLZObject & rObj, const BYTE * pbBuf, DWORD * pdwKey = nullptr) const;
		BYTE *	GetWorkMemory() const;

	private:
		BYTE *	m_pWorkMem;
};

#endif

