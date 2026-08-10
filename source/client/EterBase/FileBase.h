#ifndef __INC_ETERBASE_FILEBASE_H__
#define __INC_ETERBASE_FILEBASE_H__

#include <windows.h>

class CFileBase
{
	public:
		enum EFileMode
		{
			FILEMODE_READ = (1 << 0),
			FILEMODE_WRITE = (1 << 1)
		};

		CFileBase();
		virtual	~CFileBase();

		void			Destroy();
		void			Close();

		BOOL			Create(const char* filename, EFileMode mode);
		DWORD			Size() const;
		void			SeekCur(DWORD size) const;
		void			Seek(DWORD offset) const;
		DWORD			GetPosition() const;

		virtual BOOL	Write(const void* src, int bytes);
		BOOL			Read(void* dest, int bytes) const;

		char*			GetFileName();
		BOOL			IsNull() const;

	protected:
		int				m_mode;
		char			m_filename[MAX_PATH+1];
		HANDLE			m_hFile;
		DWORD			m_dwSize;
};

#endif

