CC = gcc
CXX = g++

DB_VERSION = $(shell cat ../../__REVISION__)
LBITS := $(shell getconf LONG_BIT)

INCDIR =
LIBDIR =
BINDIR = ..
OBJDIR = .obj
$(shell if [ ! -d $(OBJDIR) ]; then mkdir $(OBJDIR); fi)

# LIST_OF_CONSTANTS BEGIN
ENABLE_GCC_AUTODEPEND = 1
ENABLE_STATIC = 0
ENABLE_ASAN = 0
# LIST_OF_CONSTANTS END

# Depend Path File
ifneq ($(ENABLE_GCC_AUTODEPEND), 1)
DEPFILE = Depend
endif

# Project Flags
CFLAGS  = -m32 -g -Wall -O2 -pipe -fno-rtti -fno-exceptions -Wno-long-long -pthread -D_THREAD_SAFE -include bsd/string.h
CFLAGS += -Wno-deprecated-declarations -Wno-nonnull-compare -Wno-deprecated-declarations -Wno-array-bounds -Wno-address
CFLAGS += -Wno-int-in-bool-context -Wno-format-truncation -Wno-deprecated-enum-enum-conversion
CXXFLAGS = -std=c++20

# for clang
ifneq '' '$(findstring clang++,$(CXX))'
CFLAGS += -Wno-invalid-source-encoding -Wno-deprecated-anon-enum-enum-conversion -Wno-unknown-warning-option
# for gcc
else ifneq '' '$(findstring g++,$(CXX))'
# for 32bit on 64bit
ifeq ($(LBITS),64)
#CFLAGS += -L/usr/local/lib32/gcc10
#CFLAGS += -Wl,-rpath=/usr/local/lib32/gcc10
else
# for 32bit on 32bit
#CXXFLAGS += -Wl,-rpath=/usr/local/lib/gcc10
endif
endif

ifeq ($(ENABLE_ASAN), 1)
CFLAGS += -DENABLE_ASAN
CFLAGS += -fsanitize=address -fno-omit-frame-pointer
CFLAGS += -fsanitize-recover=address
endif

# MySQL
INCDIR += -I/usr/include/mariadb
ifeq ($(LBITS),64)
LIBS += -lmariadb -lz
else
LIBS += -lmariadb -lz
endif

ifeq ($(ENABLE_STATIC), 1)
CFLAGS += -static
endif

# Version defines
CFLAGS += -D__USER__=\"$(USER)\" -D__HOSTNAME__=\"$(HOSTNAME)\" -D__PWD__=\"$(PWD)\" -D__DB_VERSION__=\"$(DB_VERSION)\"

# Boost
INCDIR += -I../../../Extern/include/boost

# Project Libraries
INCDIR += -I/usr/local/include
INCDIR += -I../../../Extern/include
LIBDIR += -I../../../Extern/lib

LIBDIR += -L../../libthecore/lib -L../../libsql -L../../libpoly -L../../libgame/lib
LIBS += -lthecore -lsql -lpoly -lgame -lm -lbsd

# OpenSSL
ifeq ($(LBITS),64)
LIBS += -lcrypto -lssl
else
LIBS += -lssl -lcrypto
endif

# PROJECT_SRC_FILES BEGIN
CPPFILE = Config.cpp NetBase.cpp Peer.cpp PeerBase.cpp Main.cpp DBManager.cpp \
		  Cache.cpp LoginData.cpp ClientManager.cpp ClientManagerPlayer.cpp ClientManagerLogin.cpp \
		  ClientManagerBoot.cpp ClientManagerParty.cpp ClientManagerGuild.cpp GuildManager.cpp \
		  PrivManager.cpp MoneyLog.cpp ItemAwardManager.cpp ClientManagerEventFlag.cpp Marriage.cpp \
		  Monarch.cpp ItemIDRangeManager.cpp ClientManagerHorseName.cpp version.cpp \
		  ProtoReader.cpp CsvReader.cpp
# PROJECT_SRC_FILES END

# PROJECT_OBJ_FILES BEGIN
CPPOBJS = $(CPPFILE:%.cpp=$(OBJDIR)/%.o)
ifeq ($(ENABLE_GCC_AUTODEPEND), 1)
CPPDEPS	= $(CPPOBJS:%.o=%.d)
endif
# PROJECT_OBJ_FILES END

# Target Paths
MAIN_TARGET = $(BINDIR)/db_r$(DB_VERSION)

default: $(MAIN_TARGET)

$(MAIN_TARGET): $(CPPOBJS)
	@echo linking $(MAIN_TARGET)
	@$(CXX) $(CFLAGS) $(CXXFLAGS) $(LIBDIR) $(CPPOBJS) $(LIBS) -o $(MAIN_TARGET)

$(OBJDIR)/%.o: %.cpp
	@echo compiling $<
	@$(CXX) $(CFLAGS) $(CXXFLAGS) $(INCDIR) -c $< -o $@
ifeq ($(ENABLE_GCC_AUTODEPEND), 1)
	@$(CXX) -MM -MG -MP $(CFLAGS) $(CXXFLAGS) $(INCDIR) -c $< -o $(OBJDIR)/$*.d
	@sed -i -e's/$*.o:/$(OBJDIR)\/$*.o:/g' $(OBJDIR)/$*.d
endif

$(OBJDIR):
	@mkdir $(OBJDIR)

symlink:
	@ln -fs db_r$(DB_VERSION) $(BINDIR)/db_symlink

strip:
	@cp $(BINDIR)/db_r$(DB_VERSION) $(BINDIR)/db_r
	@strip $(BINDIR)/db_r

clean:
	@rm -f $(CPPOBJS) $(BINDIR)/db_r*

dep:
ifeq ($(ENABLE_GCC_AUTODEPEND), 1)
	@echo "Note: gcc autodepend is autodetected, so target dep skipped"
else
	makedepend -f $(DEPFILE) $(INCDIR) -I/usr/include/c++/3.3 -I/usr/include/c++/4.2 -p$(OBJDIR)/ $(CPPFILE) 2> /dev/null > $(DEPFILE)
endif

# AUTO_DEPEND_CHECK BEGIN
ifeq ($(ENABLE_GCC_AUTODEPEND), 1)
sinclude $(CPPDEPS)
else
sinclude $(DEPFILE)
endif
# AUTO_DEPEND_CHECK END
