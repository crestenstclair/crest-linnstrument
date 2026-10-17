#pragma once
#include "Arduino.h"
constexpr int SPI_CONTINUE=1, SPI_MODE0=0;
class SpiBus {
public:
    void begin(int);
    void setDataMode(int,int);
    void setClockDivider(int,int);
    byte transfer(int,byte,int=0);
};
extern SpiBus SPI;
