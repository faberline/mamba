#define _GNU_SOURCE 1
#include <arpa/inet.h>
#include <assert.h>
#include <limits.h>
#include <netdb.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/socket.h>

/* Independent host-header/OS control, not a substitute for Mamba tests. */
int main(void) {
    struct sockaddr_in v4 = {0};
    struct sockaddr_in6 v6 = {0};
    v4.sin_family = AF_INET;
    v4.sin_port = htons(0x1234);
    v6.sin6_family = AF_INET6;
    v6.sin6_port = htons(443);
#ifndef __linux__
    v4.sin_len = sizeof(v4);
    v6.sin6_len = sizeof(v6);
    assert(v4.sin_len == sizeof(v4) && v6.sin6_len == sizeof(v6));
#endif
    assert(inet_pton(AF_INET, "127.0.0.1", &v4.sin_addr) == 1);
    assert(inet_pton(AF_INET6, "::1", &v6.sin6_addr) == 1);
    assert(ntohs(v4.sin_port) == 0x1234);
    char host[128], service[32];
    assert(getnameinfo((const struct sockaddr *)&v6, sizeof(v6),
                       host, sizeof(host), service, sizeof(service),
                       NI_NUMERICHOST | NI_NUMERICSERV) == 0);
    assert(strcmp(host, "::1") == 0 && strcmp(service, "443") == 0);
    _Static_assert((int64_t)RLIM_NLIMITS > 0 && (int64_t)RLIM_NLIMITS <= INT_MAX,
                   "resource selector bound must fit the Python C-int contract");
    struct rlimit limits;
    assert(getrlimit(RLIMIT_NOFILE, &limits) == 0);
#ifdef __linux__
    printf("{\"linux\":true,\"pollmsg\":%d,", POLLMSG);
#else
    printf("{\"linux\":false,\"pollmsg\":null,");
#endif
    printf("\"ipv6_host\":\"%s\",\"ipv6_service\":\"%s\","
           "\"rlim_nlimits\":%lld,\"sockaddr_v4_size\":%zu,\"sockaddr_v6_size\":%zu}\n",
           host, service, (long long)RLIM_NLIMITS, sizeof(v4), sizeof(v6));
    return 0;
}
