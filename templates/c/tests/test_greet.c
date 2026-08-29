#include "../src/greet.h"

#include <assert.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    char buf[256];
    greet(buf, sizeof buf, "world");
    assert(strcmp(buf, "hello from __NAME__, world") == 0);
    printf("ok: greet\n");
    return 0;
}
