#ifndef __IDENT___GREET_H
#define __IDENT___GREET_H

/* Writes "hello from __NAME__, <name>" into buf. Returns buf. */
char *greet(char *buf, unsigned long buflen, const char *name);

#endif
