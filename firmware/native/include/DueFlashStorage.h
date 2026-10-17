#pragma once
#include "Arduino.h"
class DueFlashStorage {
public:
    byte read(uint32_t);
    byte* readAddress(uint32_t);
    bool write(uint32_t,byte);
    bool write(uint32_t,byte*,uint32_t);
};
