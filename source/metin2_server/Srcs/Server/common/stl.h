#ifndef __INC_METIN_II_STL_H__
#define __INC_METIN_II_STL_H__

#include <vector>
#include <string>
#include <algorithm>
#include <random>
#include <map>
#include <list>
#include <functional>
#include <stack>
#include <set>

#ifdef __GNUC__
	#ifndef __clang__
		#include <ext/functional>
	#else
		#include <functional>
	#endif
#endif

#ifndef itertype
#define itertype(v) typeof((v).begin())
#endif

inline void stl_lowers(std::string& rstRet)
{
	for (size_t i = 0; i < rstRet.length(); ++i)
		rstRet[i] = tolower(rstRet[i]);
}

template <typename T, std::size_t N>
void copy_array_to_std_array(const T(&arr)[N], std::array<T, N>& std_arr) {
	std::copy(arr, arr + N, std_arr.begin());
}

struct stringhash
{
	size_t operator () (const std::string & str) const
	{
		const unsigned char * s = (const unsigned char*) str.c_str();
		const unsigned char * end = s + str.size();
		size_t h = 0;

		while (s < end)
		{
			h *= 16777619;
			h ^= *(s++);
		}

		return h;
	}
};

// code from tr1/functional_hash.h
template<typename T>
struct hash;

template<typename _Tp>
struct hash<_Tp*>
{
	std::size_t
		operator()(_Tp* __p) const
		{ return reinterpret_cast<std::size_t>(__p); }
};

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

namespace msl
{
	template< class F, class... Args >
	constexpr decltype(auto) bind1st( F&& f, Args&&... args )
	{
		return std::bind(f, args..., std::placeholders::_1);
	}

	template< class F, class... Args >
	constexpr decltype(auto) bind2nd( F&& f, Args&&... args )
	{
		return std::bind(f, std::placeholders::_1, args...);
	}

	template <class T>
	void random_shuffle(T first, T last)
	{
		std::random_device rd;
		std::mt19937 g(rd());
		std::shuffle(first, last, g);
	}
};

namespace std
{
	template <class container, class Pred>
		void erase_if (container & a, typename container::iterator first, typename container::iterator past, Pred pred)
		{
			while (first != past)
				if (pred(*first))
					a.erase(first++);
				else
					++first;
		}

	template <class container>
		void wipe(container & a)
		{
			typename container::iterator first, past;

			first = a.begin();
			past = a.end();

			while (first != past)
				delete *(first++);

			a.clear();
		}

	template <class container>
		void wipe_second(container & a)
		{
			typename container::iterator first, past;

			first = a.begin();
			past = a.end();

			while (first != past)
			{
				delete first->second;
				++first;
			}

			a.clear();
		}

	template <typename T> T MINMAX(T min, T value, T max)
	{
		T tv;

		tv = (min > value ? min : value);
		return (max < tv) ? max : tv;
	}

	template <class _Ty>
		class void_mem_fun_t
		{
			public:
				explicit void_mem_fun_t(void (_Ty::*_Pm)()) : _Ptr(_Pm)
				{
				}

				void operator()(_Ty* p) const
				{
					((p->*_Ptr)());
				}

			private:
				void (_Ty::*_Ptr)();
		};

	template<class _Ty> inline
		void_mem_fun_t<_Ty> void_mem_fun(void (_Ty::*_Pm)())
		{ return (void_mem_fun_t<_Ty>(_Pm)); }

	template<class _Ty>
		class void_mem_fun_ref_t
		{
			public:
				explicit void_mem_fun_ref_t(void (_Ty::*_Pm)()) : _Ptr(_Pm) {}
				void operator()(_Ty& x) const
				{ return ((x.*_Ptr)()); }
			private:
				void (_Ty::*_Ptr)();
		};

	template<class _Ty> inline
		void_mem_fun_ref_t<_Ty> void_mem_fun_ref(void (_Ty::*_Pm)())
		{ return (void_mem_fun_ref_t< _Ty>(_Pm)); }
};

inline std::string FormatNumberWithDots(long long number) {
	std::string result;
	bool isNegative = false;

	// Handle the case for negative numbers
	if (number < 0) {
		isNegative = true;
		number = -number; // Make it positive for processing
	}

	// Convert the number to a string
	std::string numStr = std::to_string(number);

	// Insert dots from the right every three digits
	int count = 0;
	for (int i = numStr.size() - 1; i >= 0; --i) {
		result.push_back(numStr[i]);
		if (++count == 3 && i > 0) {
			result.push_back('.');
			count = 0;
		}
	}

	// Reverse the result string to get the correct order
	std::reverse(result.begin(), result.end());

	// Add the negative sign if necessary
	if (isNegative) {
		result.insert(result.begin(), '-');
	}

	return result;
}

#endif

