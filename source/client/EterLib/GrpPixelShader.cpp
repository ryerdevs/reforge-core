#include "StdAfx.h"
#include "GrpPixelShader.h"
#include "GrpD3DXBuffer.h"
#include "StateManager.h"

CPixelShader::CPixelShader()
{
	Initialize();
}

CPixelShader::~CPixelShader()
{
	Destroy();
}

void CPixelShader::Initialize()
{
	m_handle=0;
}

void CPixelShader::Destroy()
{
	if (m_handle)
	{
		m_handle=0;
	}
}

bool CPixelShader::CreateFromDiskFile(const char* c_szFileName)
{
	Destroy();

	LPD3DXBUFFER lpd3dxShaderBuffer;
	LPD3DXBUFFER lpd3dxErrorBuffer;

	if (FAILED(
		D3DXAssembleShaderFromFileA(c_szFileName, 0, nullptr, 0, &lpd3dxShaderBuffer, &lpd3dxErrorBuffer)
	))
		return false;

	if (FAILED(ms_lpd3dDevice->CreatePixelShader((const DWORD*)lpd3dxShaderBuffer->GetBufferPointer(), &m_handle)))
		return false;

	return true;
}

void CPixelShader::Set() const
{
	STATEMANAGER.SetPixelShader(m_handle);
}

