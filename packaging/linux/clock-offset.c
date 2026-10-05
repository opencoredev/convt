/* Test-only LD_PRELOAD shim: moves the wall clock forward by
 * CONVT_CLOCK_OFFSET_SECONDS so reproducibility checks can simulate building
 * on another day. container-build.sh compiles it only when
 * CONVT_BUILD_CLOCK_OFFSET_DAYS is set; it never enters the payload. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdlib.h>
/* glibc versions disagree on the second parameter type; hide the prototype. */
#define gettimeofday convt_hidden_gettimeofday
#include <sys/time.h>
#undef gettimeofday
#include <time.h>

static long offset(void) {
    const char *s = getenv("CONVT_CLOCK_OFFSET_SECONDS");
    return s ? atol(s) : 0;
}

int clock_gettime(clockid_t id, struct timespec *ts) {
    static int (*real)(clockid_t, struct timespec *);
    if (!real) real = (int (*)(clockid_t, struct timespec *))dlsym(RTLD_NEXT, "clock_gettime");
    int r = real(id, ts);
    if (r == 0 && (id == CLOCK_REALTIME || id == CLOCK_REALTIME_COARSE)) ts->tv_sec += offset();
    return r;
}

int gettimeofday(struct timeval *tv, void *tz) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts)) return -1;
    if (tv) { tv->tv_sec = ts.tv_sec; tv->tv_usec = ts.tv_nsec / 1000; }
    (void)tz;
    return 0;
}

time_t time(time_t *t) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    if (t) *t = ts.tv_sec;
    return ts.tv_sec;
}
