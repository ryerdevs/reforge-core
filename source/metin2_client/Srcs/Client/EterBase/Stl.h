#ifndef __INC_ETERBASE_STL_H__
#define __INC_ETERBASE_STL_H__

#pragma warning(disable:4786)	// identifier was truncated to '255' characters in the browser information
#pragma warning(disable:4018)	// signed <-> unsigned mismatch
#pragma warning(disable:4503)	// decorated name length exceeded, name was truncated
#pragma warning(disable:4018)	// '<' : signed/unsigned mismatch

#include <assert.h>

#pragma warning ( push, 3 )

#include <algorithm>
#include <string>
#include <array>
#include <vector>
#include <stack>
#include <deque>
#include <list>
#include <set>
#include <map>
#include <queue>
#include <functional>
#include <unordered_map>
#include <unordered_set>
#include <SSTREAM>

#pragma warning ( pop )

extern char korean_tolower(const char c);
extern std::string& stl_static_string(const char* c_sz);
extern void stl_lowers(std::string& rstRet);
extern int split_string(const std::string & input, const std::string & delimiter, std::vector<std::string>& results, bool includeEmpties);

namespace utils
{
	template <typename T> struct IsContiguous {
		static bool constexpr value = false;
	};

	template <typename... Args>
	struct IsContiguous<std::vector<Args...>> {
		static bool constexpr value = true;
	};

	template <>
	struct IsContiguous<std::string> {
		static bool constexpr value = true;
	};

	template <typename... Args>
	struct IsContiguous<std::array<Args...>> {
		static bool constexpr value = true;
	};

	template<typename T>
	constexpr bool IsContiguousV = IsContiguous<T>::value;

	/////////////////////
	template<typename T>
	constexpr bool IsRawV = !std::is_pointer_v<T> && std::is_trivially_copyable_v<T> && !IsContiguousV<T>;
};

namespace std
{
#if _HAS_CXX17
	// FUNCTIONAL STUFF (from <functional>)
	// STRUCT TEMPLATE unary_function
	template <class _Arg, class _Result> struct unary_function
	{ // base class for unary functions
		typedef _Arg argument_type;
		typedef _Result result_type;
	};

	// STRUCT TEMPLATE binary_function
	template <class _Arg1, class _Arg2, class _Result> struct binary_function
	{ // base class for binary functions
		typedef _Arg1 first_argument_type;
		typedef _Arg2 second_argument_type;
		typedef _Result result_type;
	};
#endif /* _HAS_AUTO_PTR_ETC */

	template <class _Ty>
	class void_mem_fun_t
		: public unary_function<_Ty *, void> {
	public:
		explicit void_mem_fun_t(void (_Ty::*_Pm)())
			: _Ptr(_Pm) {}
		void operator()(_Ty *_P) const
		{((_P->*_Ptr)()); }
	private:
		void (_Ty::*_Ptr)();
		};
	template<class _Ty> inline
	void_mem_fun_t<_Ty> void_mem_fun(void (_Ty::*_Pm)())
	{return (void_mem_fun_t<_Ty>(_Pm)); }

	template<class _Ty>
	class void_mem_fun_ref_t : public unary_function<_Ty, void> {
	public:
		explicit void_mem_fun_ref_t(void (_Ty::*_Pm)())
			: _Ptr(_Pm) {}
		void operator()(_Ty& _X) const
		{return ((_X.*_Ptr)()); }
	private:
		void (_Ty::*_Ptr)();
	};

	template<class _Ty> inline
	void_mem_fun_ref_t<_Ty> void_mem_fun_ref(void (_Ty::*_Pm)())
	{return (void_mem_fun_ref_t< _Ty>(_Pm)); }

		// TEMPLATE CLASS mem_fun1_t
template<class _R, class _Ty, class _A>
	class void_mem_fun1_t : public binary_function<_Ty *, _A, _R> {
public:
	explicit void_mem_fun1_t(_R (_Ty::*_Pm)(_A))
		: _Ptr(_Pm) {}
	_R operator()(_Ty *_P, _A _Arg) const
		{return ((_P->*_Ptr)(_Arg)); }
private:
	_R (_Ty::*_Ptr)(_A);
	};
		// TEMPLATE FUNCTION mem_fun1
template<class _R, class _Ty, class _A> inline
	void_mem_fun1_t<_R, _Ty, _A> void_mem_fun1(_R (_Ty::*_Pm)(_A))
	{return (void_mem_fun1_t<_R, _Ty, _A>(_Pm)); }
}

struct stl_sz_less
{
	bool operator() (char * const & left, char * const & right) const
	{
		return (strcmp(left, right) < 0);
	}
};

template<typename TContainer>
inline void stl_wipe(TContainer& container)
{
	for (auto i = container.begin(); i != container.end(); ++i)
	{
		delete *i;
		*i = nullptr;
	}

	container.clear();
}

template<typename TString>
inline int hex2dec(TString szhex)
{
	const int hex0 = toupper(szhex[0]);
	const int hex1 = toupper(szhex[1]);

	return (hex1 >= 'A' ? hex1 - 'A' + 10 : hex1 - '0') +
		   (hex0 >= 'A' ? hex0 - 'A' + 10 : hex0 - '0') * 16;
}

template<typename TString>
inline unsigned long htmlColorStringToARGB(TString str)
{
	const unsigned long alp	= hex2dec(str);
	const unsigned long red	= hex2dec(str + 2);
	const unsigned long green	= hex2dec(str + 4);
	const unsigned long blue	= hex2dec(str + 6);
	return (alp << 24 | red << 16 | green << 8 | blue);
}

template<typename TContainer>
inline void stl_wipe_second(TContainer& container)
{
	for (auto i = container.begin(); i != container.end(); ++i)
	{
		delete i->second;
	}

	container.clear();
}

template<typename T>
inline void safe_release(T& rpObject)
{
	if (!rpObject)
		return;

	rpObject->Release();
	rpObject = nullptr;
}

template <typename T>
void DeleteVectorItem(std::vector<T> * pVector, unsigned long dwIndex)
{
	if (dwIndex >= pVector->size())
	{
		assert(!"Wrong index to delete!");
		return;
	}
	if (1 == pVector->size())
	{
		pVector->clear();
		return;
	}

	auto itor = pVector->begin();
	for (unsigned long i = 0; i < dwIndex; ++i)
		++itor;

	pVector->erase(itor);
}

template <typename T>
void DeleteVectorItem(T * pVector, unsigned long dwStartIndex, unsigned long dwEndIndex)
{
	if (dwStartIndex >= pVector->size())
	{
		assert(!"Wrong start index to delete!");
		return;
	}
	if (dwEndIndex >= pVector->size())
	{
		assert(!"Wrong end index to delete!");
		return;
	}

	auto itorStart = pVector->begin();
	for (unsigned long i = 0; i < dwStartIndex; ++i)
		++itorStart;
	auto itorEnd = pVector->begin();
	for (unsigned long j = 0; j < dwEndIndex; ++j)
		++itorEnd;

	pVector->erase(itorStart, itorEnd);
}

template <typename T>
void DeleteVectorItem(std::vector<T> * pVector, T pItem)
{
	auto itor = pVector->begin();
	for (; itor != pVector->end(); ++itor)
	{
		if (pItem == *itor)
		{
			if (1 == pVector->size())
			{
				pVector->clear();
			}
			else
			{
				pVector->erase(itor);
			}
			break;
		}
	}
}

template <typename T>
void DeleteListItem(std::list<T> * pList, T pItem)
{
	auto itor = pList->begin();
	for (; itor != pList->end(); ++itor)
	{
		if (pItem == *itor)
		{
			if (1 == pList->size())
			{
				pList->clear();
			}
			else
			{
				pList->erase(itor);
			}
			break;
		}
	}
}

template<typename T, typename F>
void stl_vector_qsort(std::vector<T>& rdataVector, F comp)
{
	if (rdataVector.empty()) return;
	qsort(&rdataVector[0], rdataVector.size(), sizeof(T), comp);
}

template<typename TData>
class stl_stack_pool
{
	public:
		stl_stack_pool()
		{
			m_pos = 0;
		}

		stl_stack_pool(int capacity)
		{
			m_pos = 0;
			initialize(capacity);
		}

		virtual ~stl_stack_pool()
		{
		}

		void initialize(int capacity)
		{
			m_dataVector.clear();
			m_dataVector.resize(capacity);
		}

		void clear()
		{
			m_pos = 0;
		}

		TData * alloc()
		{
			assert(!m_dataVector.empty() && "stl_stack_pool::alloc you MUST run stl_stack_pool::initialize");

			const int max = m_dataVector.size();

			if (m_pos >= max)
			{
				assert(!"stl_stack_pool::alloc OUT of memory");
				m_pos = 0;
			}

			return &m_dataVector[m_pos++];
		}

		TData* base()
		{
			return &m_dataVector[0];
		}

		int size() const
		{
			return m_pos;
		}

	private:
		int m_pos;

		std::vector<TData>	m_dataVector;
};

template<typename TData, typename THandle=int>
class stl_circle_pool
{
	public:
		typedef bool TFlag;

	public:
		stl_circle_pool()
		{
			initialize();
		}
		virtual ~stl_circle_pool()
		{
			destroy();
		}
		void destroy()
		{
			if (m_datas)
			{
				delete [] m_datas;
				m_datas=nullptr;
			}
			if (m_flags)
			{
				delete [] m_flags;
				m_flags= nullptr;
			}
		}
		void create(int size)
		{
			destroy();

			initialize();

			m_size=size;
			m_datas=new TData[m_size];
			m_flags=new TFlag[m_size];

			for (int i=0; i<m_size; ++i)
				m_flags[i]=false;
		}
		THandle alloc()
		{
			THandle max=m_size;

			THandle loop=max;
			while (loop--)
			{
				int cur=m_pos%max;++m_pos;
				if (!m_flags[cur])
				{
					m_flags[cur]=true;
					return cur;
				}
			}

			assert(!"Out of Memory");

			return 0;
		}
		void free(THandle handle)
		{
			assert(check(handle) && "Out of RANGE");
			m_flags[handle]=false;
		}
		inline bool check(THandle handle)
		{
			if (handle>=m_size) return false;
			return true;
		}
		inline int size()
		{
			return m_size;
		}
		inline TData& refer(THandle handle)
		{
			assert(check(handle) && "Out of RANGE");
			return m_datas[handle];
		}

	protected:
		void initialize()
		{
			m_datas=nullptr;
			m_flags= nullptr;
			m_pos=0;
			m_size=0;
		}

	protected:
		TData*	m_datas;
		TFlag*	m_flags;

		THandle m_size;

		THandle m_pos;
};

typedef std::vector<std::string> CTokenVector;
typedef std::map<std::string, std::string> CTokenMap;
typedef std::map<std::string, CTokenVector> CTokenVectorMap;

struct stringhash
{
	size_t GetHash(const std::string & str) const
	{
		auto s = (const unsigned char*) str.c_str();
       const unsigned char * end = s + str.size();
       size_t h = 0;

       while (s < end)
       {
           h *= 16777619;
           h ^= (unsigned char) *(unsigned char *) (s++);
       }

       return h;
	}

    size_t operator () (const std::string & str) const
    {
		return GetHash(str);
    }
};

#endif

