#ifndef __INC_METIN_II_GAME_BUFFER_MANAGER_H__
#define __INC_METIN_II_GAME_BUFFER_MANAGER_H__
#include "../../common/stl.h"

class TEMP_BUFFER
{
	public:
		TEMP_BUFFER(int Size = 8192, bool ForceDelete = false );
		~TEMP_BUFFER();

		const void * 	read_peek() const;

		template<typename T, std::enable_if_t<utils::IsRawV<T>>* = nullptr>
		void write(const T& c_pvData) {
			write(&c_pvData, sizeof(T));
		}
		template<typename C, std::enable_if_t<utils::IsContiguousV<C>>* = nullptr>
		void write(const C& v) {
			write(v.data(), v.size() * sizeof(typename C::value_type));
		}

		void		write(const void * data, int size);
		int		size() const;
		void	reset() const;

		LPBUFFER	getptr() const { return buf; }

	protected:
		LPBUFFER	buf;
		bool		forceDelete;
};

#endif

