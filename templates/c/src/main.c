#include "greet.h"

#include <stdio.h>

int main(int argc, char **argv) {
    char buf[256];
    const char *name = argc > 1 ? argv[1] : "world";
    printf("%s\n", greet(buf, sizeof buf, name));
    return 0;
}
