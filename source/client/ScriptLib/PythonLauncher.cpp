#include "StdAfx.h"
#include <Python27/frameobject.h>
#include "../eterPack/EterPackManager.h"

#include "PythonLauncher.h"

CPythonLauncher::CPythonLauncher()
{
	Py_Initialize();
}

CPythonLauncher::~CPythonLauncher()
{
	Clear();
}

void CPythonLauncher::Clear() const
{
	Py_Finalize();
}

std::string g_stTraceBuffer[512];
int	g_nCurTraceN = 0;

void Traceback()
{
	std::string str;

	for (int i = 0; i < g_nCurTraceN; ++i)
	{
		str.append(g_stTraceBuffer[i]);
		str.append("\n");
	}

	PyObject * exc;
	PyObject * v;
	PyObject * tb;
	const char * errStr;

	PyErr_Fetch(&exc, &v, &tb);

	if (PyString_Check(v))
	{
		errStr = PyString_AS_STRING(v);
		str.append("Error: ");
		str.append(errStr);

		Tracef("%s\n", errStr);
	}
	Py_DECREF(exc);
	Py_DECREF(v);
	Py_DECREF(tb);
	AppendPythonErrorLog(str.c_str());	// F4 instrumentación: traceback completo al log
	LogBoxf("Traceback:\n\n%s\n", str.c_str());
}

int TraceFunc(PyObject * obj, PyFrameObject * f, int what, PyObject *arg)
{
	const char * funcname;
	char szTraceBuffer[128];

	switch (what)
	{
		case PyTrace_CALL:
			if (g_nCurTraceN >= 512)
				return 0;

			if (Py_OptimizeFlag)
				f->f_lineno = PyCode_Addr2Line(f->f_code, f->f_lasti);

			funcname = PyString_AsString(f->f_code->co_name);

			_snprintf(szTraceBuffer, sizeof(szTraceBuffer), "Call: File \"%s\", line %d, in %s",
					  PyString_AsString(f->f_code->co_filename),
					  f->f_lineno,
					  funcname);

			g_stTraceBuffer[g_nCurTraceN++]=szTraceBuffer;
			break;

		case PyTrace_RETURN:
			if (g_nCurTraceN > 0)
				--g_nCurTraceN;
			break;

		case PyTrace_EXCEPTION:
			if (g_nCurTraceN >= 512)
				return 0;

			PyObject * exc_type, * exc_value, * exc_traceback;

			PyTuple_GetObject(arg, 0, &exc_type);
			PyTuple_GetObject(arg, 1, &exc_value);
			PyTuple_GetObject(arg, 2, &exc_traceback);

			int len;
			const char * exc_str;
			PyObject_AsCharBuffer(exc_type, &exc_str, &len);

			_snprintf(szTraceBuffer, sizeof(szTraceBuffer), "Exception: File \"%s\", line %d, in %s",
					  PyString_AS_STRING(f->f_code->co_filename),
					  f->f_lineno,
					  PyString_AS_STRING(f->f_code->co_name));

			g_stTraceBuffer[g_nCurTraceN++]=szTraceBuffer;

			break;
	}
	return 0;
}

void CPythonLauncher::SetTraceFunc(int (*pFunc)(PyObject * obj, PyFrameObject * f, int what, PyObject *arg)) const
{
	PyEval_SetTrace(pFunc, nullptr);
}

bool CPythonLauncher::Create(const char* c_szProgramName)
{
	Py_SetProgramName((char*)c_szProgramName);
	#if defined(_DEBUG) || defined(ENABLE_BL_TRACEBACK)
	SetTraceFunc(TraceFunc);
	#endif
	m_poModule = PyImport_AddModule((char *) "__main__");

	if (!m_poModule)
		return false;

	m_poDic = PyModule_GetDict(m_poModule);

    PyObject * builtins = PyImport_ImportModule("__builtin__");
	PyModule_AddIntConstant(builtins, "TRUE", 1);
	PyModule_AddIntConstant(builtins, "FALSE", 0);
    PyDict_SetItemString(m_poDic, "__builtins__", builtins);
	Py_DECREF(builtins);

	if (!RunLine("import __main__"))
		return false;

	if (!RunLine("import sys"))
		return false;

	return true;
}

#ifndef __USE_CYTHON__
bool CPythonLauncher::RunCompiledFile(const char* c_szFileName) const
{
	FILE * fp = fopen(c_szFileName, "rb");

	if (!fp)
		return false;

	PyCodeObject *co;
	PyObject *v;
	long magic;
	long PyImport_GetMagicNumber(void);

	magic = _PyMarshal_ReadLongFromFile(fp);

	if (magic != PyImport_GetMagicNumber())
	{
		PyErr_SetString(PyExc_RuntimeError, "Bad magic number in .pyc file");
		fclose(fp);
		return false;
	}

	_PyMarshal_ReadLongFromFile(fp);
	v = _PyMarshal_ReadLastObjectFromFile(fp);

	fclose(fp);

	if (!v || !PyCode_Check(v))
	{
		Py_XDECREF(v);
		PyErr_SetString(PyExc_RuntimeError, "Bad code object in .pyc file");
		return false;
	}

	co = (PyCodeObject *) v;
	v = PyEval_EvalCode(co, m_poDic, m_poDic);

	Py_DECREF(co);
	if (!v)
	{
		Traceback();
		return false;
	}

	Py_DECREF(v);
	if (Py_FlushLine())
		PyErr_Clear();

	return true;
}

bool CPythonLauncher::RunMemoryTextFile(const char* c_szFileName, UINT uFileSize, const VOID* c_pvFileData) const
{
	const auto c_pcFileData=(const CHAR*)c_pvFileData;
	std::string stConvFileData;
	stConvFileData.reserve(uFileSize);

	for (UINT i=0; i<uFileSize; ++i)
	{
		if (c_pcFileData[i]!=13)
			stConvFileData+=c_pcFileData[i];
	}

	// @fixme058 BEGIN
	const auto c_pcConvFileData = stConvFileData.c_str();
	auto pCompiledCode = Py_CompileString(c_pcConvFileData, c_szFileName, Py_file_input);
	if (!pCompiledCode)
		return false;

	auto pResult = PyEval_EvalCode((PyCodeObject*)pCompiledCode, m_poDic, m_poDic);
	Py_DECREF(pCompiledCode);
	if (!pResult)
		return false;

	Py_DECREF(pResult);
	if (Py_FlushLine())
		PyErr_Clear();
	// @fixme058 END

	return true;
}

bool CPythonLauncher::RunFile(const char* c_szFileName) const
{
	std::string acBufData; // @fixme058

	{
		CMappedFile file;
		const VOID* pvData;
		CEterPackManager::Instance().Get(file, c_szFileName, &pvData);

		if (file.Size() == 0)
			return false;

		acBufData.resize(file.Size());
		memcpy(acBufData.data(), pvData, acBufData.size());
	}

	return RunMemoryTextFile(c_szFileName, acBufData.size(), acBufData.data());
}
#endif

bool CPythonLauncher::RunLine(const char* c_szSrc) const
{
	PyObject * v = PyRun_String((char *) c_szSrc, Py_file_input, m_poDic, m_poDic);

	if (!v)
	{
		Traceback();
		return false;
	}

	Py_DECREF(v);
	return true;
}

const char* CPythonLauncher::GetError() const
{
	PyObject* exc;
	PyObject* v;
	PyObject* tb;

	PyErr_Fetch(&exc, &v, &tb);

	if (PyString_Check(v))
		return PyString_AS_STRING(v);

	return "";
}

