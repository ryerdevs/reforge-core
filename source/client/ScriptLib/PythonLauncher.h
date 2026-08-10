#pragma once
#include <Python27/frameobject.h>

#include "../eterBase/Singleton.h"

class CPythonLauncher : public CSingleton<CPythonLauncher>
{
	public:
		CPythonLauncher();
		virtual ~CPythonLauncher();

		void Clear() const;

		bool Create(const char* c_szProgramName="eter.python");
		void SetTraceFunc(int (*pFunc)(PyObject * obj, PyFrameObject * f, int what, PyObject *arg)) const;
		bool RunLine(const char* c_szLine) const;
		#ifndef __USE_CYTHON__
		bool RunFile(const char* c_szFileName) const;
		bool RunMemoryTextFile(const char* c_szFileName, UINT uFileSize, const VOID* c_pvFileData) const;
		bool RunCompiledFile(const char* c_szFileName) const;
		#endif
		const char* GetError() const;

	protected:
		PyObject* m_poModule;
		PyObject* m_poDic;
};

