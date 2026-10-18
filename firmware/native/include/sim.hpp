#pragma once
#include "Arduino.h"
#include "identity.h"
namespace sim {
struct Contact { int id; double x,y,z; };
struct MidiByte { uint64_t at; byte value; };
extern uint64_t now;
extern int model;
extern std::array<byte, 262144> flash;
extern std::vector<MidiByte> midi;
extern std::deque<byte> rx;
extern std::vector<Contact> contacts;
extern uint64_t adc_reads, sensor_selects, led_writes;
void initialize(int);
void contact(Contact);
void release(int);
void foot_switch(int, bool);
void snapshot();
// Read-only display metadata from the upstream sketch, never an input path.
void surface_metadata(std::ostream&);
void led_integrate();
}
