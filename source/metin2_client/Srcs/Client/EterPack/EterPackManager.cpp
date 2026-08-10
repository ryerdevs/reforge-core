#include "StdAfx.h"

#include <io.h>
#include <assert.h>

#include "EterPackManager.h"
#include "EterPackPolicy_CSHybridCrypt.h"

#include "../eterBase/Debug.h"
#include "../eterBase/CRC32.h"

#define PATH_ABSOLUTE_YMIRWORK1	"d:/ymir work/"
#define PATH_ABSOLUTE_YMIRWORK2	"d:\\ymir work\\"

#ifdef __THEMIDA__
#include <ThemidaSDK.h>
#endif

CEterPack* CEterPackManager::FindPack(const char* c_szPathName)
{
	std::string strFileName;

	if (0 == ConvertFileName(c_szPathName, strFileName))
	{
		return &m_RootPack;
	}
	else
	{
		for (auto itor = m_DirPackMap.begin(); itor != m_DirPackMap.end(); ++itor)
		{
			const std::string & c_rstrName = itor->first;
			CEterPack * pEterPack = itor->second;

			if (CompareName(c_rstrName.c_str(), c_rstrName.length(), strFileName.c_str()))
			{
				return pEterPack;
			}
		}
	}

	return nullptr;
}

void CEterPackManager::SetCacheMode()
{
	m_isCacheMode=true;
}

void CEterPackManager::SetRelativePathMode()
{
	m_bTryRelativePath = true;
}

int CEterPackManager::ConvertFileName(const char * c_szFileName, std::string & rstrFileName) const
{
	rstrFileName = c_szFileName;
	stl_lowers(rstrFileName);

	int iCount = 0;

	for (DWORD i = 0; i < rstrFileName.length(); ++i)
	{
		if (rstrFileName[i] == '/')
			++iCount;
		else if (rstrFileName[i] == '\\')
		{
			rstrFileName[i] = '/';
			++iCount;
		}
	}

	return iCount;
}

bool CEterPackManager::CompareName(const char * c_szDirectoryName, DWORD /*dwLength*/, const char * c_szFileName) const
{
	const char * c_pszSrc = c_szDirectoryName;
	const char * c_pszCmp = c_szFileName;

	while (*c_pszSrc)
	{
		if (*(c_pszSrc++) != *(c_pszCmp++))
			return false;

		if (!*c_pszCmp)
			return false;
	}

	return true;
}

void CEterPackManager::LoadStaticCache(const char* c_szFileName)
{
	if (!m_isCacheMode)
		return;

	std::string strFileName;
	if (0 == ConvertFileName(c_szFileName, strFileName))
	{
		return;
	}

	DWORD dwFileNameHash = GetCRC32(strFileName.c_str(), strFileName.length());

	const auto f = m_kMap_dwNameKey_kCache.find(dwFileNameHash);
	if (m_kMap_dwNameKey_kCache.end() != f)
		return;

	CMappedFile kMapFile;
	const void* c_pvData;
	if (!Get(kMapFile, c_szFileName, &c_pvData))
		return;

	SCache kNewCache;
	kNewCache.m_dwBufSize = kMapFile.Size();
	kNewCache.m_abBufData = new BYTE[kNewCache.m_dwBufSize];
	memcpy(kNewCache.m_abBufData, c_pvData, kNewCache.m_dwBufSize);
	m_kMap_dwNameKey_kCache.insert(boost::unordered_map<DWORD, SCache>::value_type(dwFileNameHash, kNewCache));
}

CEterPackManager::SCache* CEterPackManager::__FindCache(DWORD dwFileNameHash)
{
	const auto f=m_kMap_dwNameKey_kCache.find(dwFileNameHash);
	if (m_kMap_dwNameKey_kCache.end()==f)
		return nullptr;

	return &f->second;
}

void	CEterPackManager::__ClearCacheMap()
{
	boost::unordered_map<DWORD, SCache>::iterator i;

	for (i = m_kMap_dwNameKey_kCache.begin(); i != m_kMap_dwNameKey_kCache.end(); ++i)
		delete [] i->second.m_abBufData;

	m_kMap_dwNameKey_kCache.clear();
}

struct TimeChecker
{
	TimeChecker(const char* name) : name(name)
	{
		baseTime = timeGetTime();
	}
	~TimeChecker()
	{
		printf("load %s (%d)\n", name, timeGetTime() - baseTime);
	}

	const char* name;
	DWORD baseTime;
};

bool CEterPackManager::Get(CMappedFile & rMappedFile, const char * c_szFileName, LPCVOID * pData)
{
	//TimeChecker timeChecker(c_szFileName);
	//Logf(1, "Load %s\n", c_szFileName);
	if (m_iSearchMode == SEARCH_FILE_FIRST)
	{
		if (GetFromFile(rMappedFile, c_szFileName, pData))
		{
			return true;
		}

		return GetFromPack(rMappedFile, c_szFileName, pData);
	}

	if (GetFromPack(rMappedFile, c_szFileName, pData))
	{
		return true;
	}

	return GetFromFile(rMappedFile, c_szFileName, pData);
}

struct FinderLock
{
	FinderLock(CRITICAL_SECTION& cs) : p_cs(&cs)
	{
		EnterCriticalSection(p_cs);
	}

	~FinderLock()
	{
		LeaveCriticalSection(p_cs);
	}

	CRITICAL_SECTION* p_cs;
};

bool CEterPackManager::GetFromPack(CMappedFile & rMappedFile, const char * c_szFileName, LPCVOID * pData)
{
	FinderLock lock(m_csFinder);

	static std::string strFileName;

	if (0 == ConvertFileName(c_szFileName, strFileName))
	{
		return m_RootPack.Get(rMappedFile, strFileName.c_str(), pData);
	}
	else
	{
		const DWORD dwFileNameHash = GetCRC32(strFileName.c_str(), strFileName.length());
		const SCache* pkCache = __FindCache(dwFileNameHash);

		if (pkCache)
		{
			rMappedFile.Link(pkCache->m_dwBufSize, pkCache->m_abBufData);
			return true;
		}

		const CEterFileDict::Item* pkFileItem = m_FileDict.GetItem(dwFileNameHash, strFileName.c_str());

		if (pkFileItem)
			if (pkFileItem->pkPack)
			{
				const bool r = pkFileItem->pkPack->Get2(rMappedFile, strFileName.c_str(), pkFileItem->pkInfo, pData);
				//pkFileItem->pkPack->ClearDataMemoryMap();
				return r;
			}
	}
#ifdef _DEBUG
	if (!strFileName.ends_with(".mss")) // @warme671 hide not found sound files
		TraceError("CANNOT_FIND_PACK_FILE [%s]", strFileName.c_str());
#endif

	return false;
}

constexpr time_t g_tCachingInterval = 10;
void CEterPackManager::ArrangeMemoryMappedPack() const
{
}

bool CEterPackManager::GetFromFile(CMappedFile & rMappedFile, const char * c_szFileName, LPCVOID * pData) const
{
	return rMappedFile.Create(c_szFileName, pData, 0, 0) ? true : false;
}

bool CEterPackManager::isExistInPack(const char * c_szFileName)
{
	std::string strFileName;

	if (0 == ConvertFileName(c_szFileName, strFileName))
	{
		return m_RootPack.IsExist(strFileName.c_str());
	}
	else
	{
		const DWORD dwFileNameHash = GetCRC32(strFileName.c_str(), strFileName.length());
		const CEterFileDict::Item* pkFileItem = m_FileDict.GetItem(dwFileNameHash, strFileName.c_str());

		if (pkFileItem)
			if (pkFileItem->pkPack)
				return pkFileItem->pkPack->IsExist(strFileName.c_str());
	}

	return false;
}

bool CEterPackManager::isExist(const char * c_szFileName)
{
	if (m_iSearchMode == SEARCH_PACK_FIRST)
	{
		if (isExistInPack(c_szFileName))
			return true;

		return _access(c_szFileName, 0) == 0 ? true : false;
	}

	//if(m_bTryRelativePath) {
	//	if (strnicmp(c_szFileName, PATH_ABSOLUTE_YMIRWORK1, strlen(PATH_ABSOLUTE_YMIRWORK1)) == 0 || strnicmp(c_szFileName, PATH_ABSOLUTE_YMIRWORK2, strlen(PATH_ABSOLUTE_YMIRWORK2)) == 0) {
	//		if(access(c_szFileName+strlen(PATH_ABSOLUTE_YMIRWORK1), 0) == 0)
	//			return true;
	//	}
	//}

	if (_access(c_szFileName, 0) == 0)
		return true;

	return isExistInPack(c_szFileName);
}

void CEterPackManager::RegisterRootPack(const char * c_szName)
{
	if (!m_RootPack.Create(m_FileDict, c_szName, ""))
	{
		TraceError("%s: Pack file does not exist", c_szName);
	}
}

const char * CEterPackManager::GetRootPackFileName() const
{
	return m_RootPack.GetDBName();
}

bool CEterPackManager::DecryptPackIV(DWORD dwPanamaKey)
{
	auto itor = m_PackMap.begin();
	while (itor != m_PackMap.end())
	{
		itor->second->DecryptIV(dwPanamaKey);
		itor++;
	}
	return true;
}

bool CEterPackManager::RegisterPackWhenPackMaking(const char * c_szName, const char * c_szDirectory, CEterPack* pPack)
{
	m_PackMap.insert(TEterPackMap::value_type(c_szName, pPack));
	m_PackList.push_front(pPack);

	m_DirPackMap.insert(TEterPackMap::value_type(c_szDirectory, pPack));
	return true;
}

bool CEterPackManager::RegisterPack(const char * c_szName, const char * c_szDirectory, const BYTE* c_pbIV)
{
	CEterPack * pEterPack = nullptr;
	{
		const auto itor = m_PackMap.find(c_szName);

		if (m_PackMap.end() == itor)
		{
			const bool bReadOnly = true;

			pEterPack = new CEterPack;
			if (pEterPack->Create(m_FileDict, c_szName, c_szDirectory, bReadOnly, c_pbIV))
			{
				m_PackMap.insert(TEterPackMap::value_type(c_szName, pEterPack));
			}
			else
			{
#ifdef _DEBUG
				Tracef("The eterpack doesn't exist [%s]\n", c_szName);
#endif
				delete pEterPack;
				pEterPack = nullptr;
				return false;
			}
		}
		else
		{
			pEterPack = itor->second;
		}
	}

	if (c_szDirectory && c_szDirectory[0] != '*')
	{
		const auto itor = m_DirPackMap.find(c_szDirectory);
		if (m_DirPackMap.end() == itor)
		{
			m_PackList.push_front(pEterPack);
			m_DirPackMap.insert(TEterPackMap::value_type(c_szDirectory, pEterPack));
		}
	}

	return true;
}

void CEterPackManager::SetSearchMode(bool bPackFirst)
{
	m_iSearchMode = bPackFirst ? SEARCH_PACK_FIRST : SEARCH_FILE_FIRST;
}

int CEterPackManager::GetSearchMode() const
{
	return m_iSearchMode;
}

CEterPackManager::CEterPackManager() : m_bTryRelativePath(false), m_iSearchMode(SEARCH_FILE_FIRST), m_isCacheMode(false)
{
	InitializeCriticalSection(&m_csFinder);
}

CEterPackManager::~CEterPackManager()
{
	__ClearCacheMap();

	auto i = m_PackMap.begin();
	const auto e = m_PackMap.end();
	while (i != e)
	{
		delete i->second;
		i++;
	}
	DeleteCriticalSection(&m_csFinder);
}

void CEterPackManager::RetrieveHybridCryptPackKeys(const BYTE *pStream)
{
	////dump file format
	//total packagecnt (4byte)
	//	for	packagecntpackage
	//		db name hash ( stl.h stringhash )
	//		extension cnt( 4byte)
	//		for	extension cnt
	//			ext hash ( stl.h stringhash )
	//			key-16byte
	//			iv-16byte
	int iMemOffset = 0;

	int		iPackageCnt;
	DWORD	dwPackageNameHash;

	memcpy( &iPackageCnt, pStream + iMemOffset, sizeof(int) );
	iMemOffset += sizeof(iPackageCnt);

	for( int i = 0; i < iPackageCnt; ++i )
	{
		int iRecvedCryptKeySize = 0;
		memcpy( &iRecvedCryptKeySize, pStream + iMemOffset, sizeof(iRecvedCryptKeySize) );
		iRecvedCryptKeySize -= sizeof(dwPackageNameHash);
		iMemOffset += sizeof(iRecvedCryptKeySize);

		memcpy( &dwPackageNameHash, pStream + iMemOffset, sizeof(dwPackageNameHash) );
		iMemOffset += sizeof(dwPackageNameHash);

		TEterPackMap::const_iterator cit;
		for( cit = m_PackMap.begin(); cit != m_PackMap.end(); ++cit )
		{
			std::string noPathName = CFileNameHelper::NoPath(string(cit->first));
			if( dwPackageNameHash == stringhash().GetHash(noPathName) )
			{
				EterPackPolicy_CSHybridCrypt* pCryptPolicy = cit->second->GetPackPolicy_HybridCrypt();
				const int iHavedCryptKeySize = pCryptPolicy->ReadCryptKeyInfoFromStream( pStream + iMemOffset );
				if (iRecvedCryptKeySize != iHavedCryptKeySize)
				{
					TraceError("CEterPackManager::RetrieveHybridCryptPackKeys	cryptokey length of file(%s) is not matched. received(%d) != haved(%d)", noPathName.c_str(), iRecvedCryptKeySize, iHavedCryptKeySize);
				}
				break;
			}
		}
		iMemOffset += iRecvedCryptKeySize;
	}
}

void CEterPackManager::RetrieveHybridCryptPackSDB( const BYTE* pStream )
{
	//cnt
	//for cnt
	//DWORD				dwPackageIdentifier;
	//DWORD				dwFileIdentifier;
	//std::vector<BYTE>	vecSDBStream;
	int iReadOffset = 0;
	int iSDBInfoCount = 0;

	memcpy( &iSDBInfoCount, pStream+iReadOffset, sizeof(int) );
	iReadOffset += sizeof(int);

	for( int i = 0; i < iSDBInfoCount; ++i )
	{
		DWORD dwPackgeIdentifier;
		memcpy( &dwPackgeIdentifier, pStream+iReadOffset, sizeof(DWORD) );
		iReadOffset += sizeof(DWORD);

		TEterPackMap::const_iterator cit;
		for( cit = m_PackMap.begin(); cit != m_PackMap.end(); ++cit )
		{
			std::string noPathName = CFileNameHelper::NoPath(string(cit->first));
			if( dwPackgeIdentifier == stringhash().GetHash(noPathName) )
			{
				EterPackPolicy_CSHybridCrypt* pCryptPolicy = cit->second->GetPackPolicy_HybridCrypt();
				iReadOffset += pCryptPolicy->ReadSupplementatyDataBlockFromStream( pStream+iReadOffset );
				break;
			}
		}
	}
}

void CEterPackManager::WriteHybridCryptPackInfo(const char* pFileName)
{
	//NOTE : this file format contains a little bit of redundant data.
	//however it`s better for seperating cryptkey & supplementary data block.

	//dump file format

	//SDB data offset(4)

	// about cryptkey
	//total packagecnt (4byte)
	//	for	packagecnt
	//		db name hash 4byte( stl.h stringhash )
	//		extension cnt( 4byte)
	//		for	extension cnt
	//			ext hash ( stl.h stringhash )
	//			key-16byte
	//			iv-16byte

	//about SDB data
	//total packagecnt (4byte)
	//	for	packagecnt
	//		db name hash 4byte( stl.h stringhash ) +child node size(4byte)
	//		sdb file cnt( 4byte )
	//		for	sdb file cnt
	//			filename hash ( stl.h stringhash )
	//			related map name size(4), relate map name
	//			sdb block size( 1byte )
	//			sdb blocks

	CFileBase keyFile;

	if( !keyFile.Create( pFileName, CFileBase::FILEMODE_WRITE) )
	{
		//TODO : write log
		return;
	}

	int iKeyPackageCount = 0;

	//write later ( SDB Offset & PackageCnt for Key )
	keyFile.SeekCur(2*sizeof(int));

	TEterPackMap::const_iterator cit;
	for( cit = m_PackMap.begin(); cit != m_PackMap.end(); ++cit )
	{
		EterPackPolicy_CSHybridCrypt* pPolicy = cit->second->GetPackPolicy_HybridCrypt();
		if( !pPolicy || !pPolicy->IsContainingCryptKey() )
			continue;

		iKeyPackageCount++;

		std::string noPathName = CFileNameHelper::NoPath(string(cit->first));

		DWORD dwPackNamehash = stringhash().GetHash(noPathName);

		CMakePackLog::GetSingleton().Writef("CEterPackManager::WriteHybridCryptPackInfo PackName : %s, Hash : %x", noPathName.c_str(), dwPackNamehash);
		keyFile.Write( &dwPackNamehash, sizeof(DWORD) );

		pPolicy->WriteCryptKeyToFile( keyFile );
	}

	//Write SDB Data
	const int iSDBDataOffset = keyFile.GetPosition();
	int iSDBPackageCnt = 0;

	//Write SDB PackageCnt Later
	keyFile.SeekCur(sizeof(int));
	for( cit = m_PackMap.begin(); cit != m_PackMap.end(); ++cit )
	{
		EterPackPolicy_CSHybridCrypt* pPolicy = cit->second->GetPackPolicy_HybridCrypt();
		if( !pPolicy || !pPolicy->IsContainingSDBFile() )
			continue;

		iSDBPackageCnt++;

		std::string noPathName = CFileNameHelper::NoPath(string(cit->first));

		DWORD dwPackNamehash = stringhash().GetHash(noPathName);
		keyFile.Write( &dwPackNamehash, sizeof(DWORD) );

		const int iSDBSizeWriteOffset = keyFile.GetPosition();
		keyFile.SeekCur(sizeof(int));

		pPolicy->WriteSupplementaryDataBlockToFile( keyFile );
		const int iSDBSizeAfterWrite = keyFile.GetPosition();

		keyFile.Seek(iSDBSizeWriteOffset);

		int iSDBSize = iSDBSizeAfterWrite-(iSDBSizeWriteOffset+4);
		keyFile.Write( &iSDBSize, sizeof(int) );

		keyFile.Seek(iSDBSizeAfterWrite);
	}

	//write sdb data start offset & package cnt
	keyFile.Seek(0);
	keyFile.Write( &iSDBDataOffset, sizeof(int));
	keyFile.Write( &iKeyPackageCount, sizeof(int));

	keyFile.Seek(iSDBDataOffset);
	keyFile.Write( &iSDBPackageCnt, sizeof(int));

	keyFile.Close();
}

