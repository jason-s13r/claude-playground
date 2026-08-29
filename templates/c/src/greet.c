#include "greet.h"

#include <stdio.h>

char *greet(char *buf, unsigned long buflen, const char *name) {
    snprintf(buf, buflen, "hello from __NAME__, %s", name);
    return buf;
}
