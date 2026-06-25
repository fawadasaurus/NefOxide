#include <stdbool.h>
#include <limits.h>

#ifndef PATH_MAX
#define PATH_MAX 1024
#endif

typedef struct Rect {
    short top;
    short left;
    short bottom;
    short right;
} Rect;

#define __OBJC__ 1
#include "Nkfl_Interface.h"
