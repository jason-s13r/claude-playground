#include "greet.h"

#include <stdio.h>
#include <string.h>

/* Overridden at build time via -DVERSION=... */
#ifndef VERSION
#define VERSION "dev"
#endif

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--version") == 0) {
        printf("__NAME__ %s\n", VERSION);
        return 0;
    }

    char buf[256];
    const char *name = argc > 1 ? argv[1] : "world";
    printf("%s\n", greet(buf, sizeof buf, name));
    return 0;
}
