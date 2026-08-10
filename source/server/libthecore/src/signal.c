#define _GNU_SOURCE
// Fallback signal constants for Linux (glibc headers may not expose them in C++ mode)
#ifndef SIGHUP
#define SIGHUP 1
#define SIGINT 2
#define SIGQUIT 3
#define SIGILL 4
#define SIGTRAP 5
#define SIGABRT 6
#define SIGBUS 7
#define SIGFPE 8
#define SIGKILL 9
#define SIGUSR1 10
#define SIGSEGV 11
#define SIGUSR2 12
#define SIGPIPE 13
#define SIGALRM 14
#define SIGTERM 15
#define SIGCHLD 17
#define SIGCONT 18
#define SIGSTOP 19
#define SIGTSTP 20
#define SIGTTIN 21
#define SIGTTOU 22
#define SIGVTALRM 26
#define SIGPROF 27
#define SIGWINCH 28
#define SIG_IGN ((void(*)(int))1)
extern "C" void (*signal(int sig, void (*func)(int)))(int);
#endif
#define __LIBTHECORE__
#include "stdafx.h"
#include <signal.h>

#ifdef __WIN32__


void signal_setup() {}
void signal_timer_disable() {}
void signal_timer_enable(int timeout_seconds) {}
#elif defined(__FreeBSD__) || defined(__linux__)
#define RETSIGTYPE void

RETSIGTYPE reap(int sig)
{
    while (waitpid(-1, NULL, WNOHANG) > 0);
    signal(SIGCHLD, reap);
}


RETSIGTYPE checkpointing(int sig)
{
    if (!tics)
    {
        sys_err("CHECKPOINT shutdown: tics did not updated.");
        if (bCheckpointCheck)
            abort();
    }
    else
		tics = 0;
}


RETSIGTYPE hupsig(int sig)
{
    shutdowned = TRUE;
    sys_log(0, "SIGHUP, SIGINT, SIGTERM signal has been received. shutting down."); // @warme012
}

RETSIGTYPE usrsig(int sig)
{
    core_dump();
}

void signal_timer_disable(void)
{
    struct itimerval itime;
    struct timeval interval;

    interval.tv_sec	= 0;
    interval.tv_usec	= 0;

    itime.it_interval = interval;
    itime.it_value = interval;

    setitimer(ITIMER_VIRTUAL, &itime, NULL);
}

void signal_timer_enable(int sec)
{
    struct itimerval itime;
    struct timeval interval;

    interval.tv_sec	= sec;
    interval.tv_usec	= 0;

    itime.it_interval = interval;
    itime.it_value = interval;

    setitimer(ITIMER_VIRTUAL, &itime, NULL);
}

void signal_setup(void)
{
    signal_timer_enable(30);

    signal(SIGVTALRM, checkpointing);

    /* just to be on the safe side: */
    signal(SIGHUP, hupsig);
    signal(SIGCHLD, reap);
    signal(SIGINT, hupsig);
    signal(SIGTERM, hupsig);
    signal(SIGPIPE, SIG_IGN);
    signal(SIGALRM, SIG_IGN);
    signal(SIGUSR1, usrsig);
}

#endif

