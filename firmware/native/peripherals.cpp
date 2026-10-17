#include "sim.hpp"
#include "SPI.h"
#include "DueFlashStorage.h"
// Arduino macros belong to the sketch, not native C++ host services.
#undef min
#undef max
#undef constrain
namespace sim {
uint64_t now=0, adc_reads=0, sensor_selects=0, led_writes=0;
int model=200;
std::array<byte,262144> flash;
std::vector<MidiByte> midi;
std::deque<byte> rx;
std::vector<Contact> contacts;
static std::array<int,64> pins;
static std::array<int,64> modes;
static uint64_t seed=1;
static uint64_t tx_ready=0;
static int baud=115200;
static int sensor_col=0, sensor_row=0, axis=2;
static std::vector<byte> sensor_bytes, led_bytes;
static uint16_t adc_value=0;
static bool adc_second=false;
static int led_col=0;
static std::array<byte,3> led_bits{};
static uint64_t led_since=0, frame_since=0;
static uint64_t exposure[26][8][3]{};
static unsigned display[26][8][3]{};
void initialize(int selected_model) {
    if(selected_model!=128 && selected_model!=200) throw std::runtime_error("invalid model");
    model=selected_model; flash.fill(255); pins.fill(HIGH); modes.fill(-1);
    frame_since=now; led_since=now;
    midi.reserve(32768); contacts.reserve(16); sensor_bytes.reserve(2); led_bytes.reserve(4);
}
void contact(Contact c) {
    const double columns=model==200?26:17;
    if(c.id<0 || !std::isfinite(c.x) || !std::isfinite(c.y) || !std::isfinite(c.z) || c.x<0 || c.x>columns-1 || c.y<0 || c.y>7 || c.z<0 || c.z>1) throw std::runtime_error("invalid contact");
    auto found=std::find_if(contacts.begin(),contacts.end(),[&](auto old){return old.id==c.id;});
    if(found!=contacts.end()) *found=c;
    else { if(contacts.size()==16) throw std::runtime_error("contact capacity"); contacts.push_back(c); }
}
void release(int id) { contacts.erase(std::remove_if(contacts.begin(),contacts.end(),[&](auto c){return c.id==id;}),contacts.end()); }
void foot_switch(int number,bool pressed) {
    if(number<1 || number>2) throw std::runtime_error("foot switch must be 1 or 2");
    pins[32+number]=pressed?LOW:HIGH;
}
static uint16_t sensor_read() {
    ++adc_reads;
    double weight=0, x=0, y=0;
    for(auto c:contacts) {
        // Ideal flat-top footprint; overlap near horizontal cell boundaries
        // lets upstream touch ownership decide whether a slide transitions.
        double w=std::clamp((0.6-std::abs(c.x-sensor_col))/0.2,0.0,1.0)*(std::lround(c.y)==sensor_row?1.0:0.0)*c.z;
        x+=c.x*w; y+=c.y*w; weight+=w;
    }
    if(axis==2) return uint16_t(4095-std::min(3500.0,weight*1500.0));
    if(weight==0) return 0;
    x/=weight; y/=weight;
    if(axis==0) return uint16_t(std::clamp((model==200?248.0:387.5)+(x-1)*(model==200?157.0:245.3125),0.0,4095.0));
    constexpr int low[]={243,781,1299,1810,2281,2718,3187,3599};
    constexpr int high[]={473,991,1486,1965,2449,2925,3401,3851};
    const double local=std::clamp(y-sensor_row+0.5,0.0,1.0);
    return uint16_t(low[sensor_row]+local*(high[sensor_row]-low[sensor_row]));
}
void led_integrate() {
    if(pins[37]==LOW) {
        uint64_t elapsed=now-led_since;
        for(int row=0;row<8;++row) for(int rgb=0;rgb<3;++rgb) if(led_bits[rgb]&(1<<row)) exposure[led_col][row][rgb]+=elapsed;
    }
    led_since=now;
}
void snapshot() {
    led_integrate();
    std::cout<<"{\"version\":1,\"firmware\":\"54bb1c9d355b1075b474ade0a0246b80ff7f3524\",\"at_us\":"<<now<<",\"build_hash\":\""<<SIM_BUILD_HASH<<"\",\"patch_hash\":\""<<SIM_PATCH_HASH<<"\",\"model\":"<<model<<",\"midi\":[";
    bool first=true;
    for(auto b:midi) { if(b.at>now) break; if(!first) std::cout<<','; first=false; std::cout<<'['<<b.at<<','<<int(b.value)<<']'; }
    std::cout<<"],\"leds\":[";
    const uint64_t duration=std::max(uint64_t(1),now-frame_since);
    int columns=model==200?26:17;
    for(int col=0;col<columns;++col) for(int row=0;row<8;++row) {
        if(col||row) std::cout<<',';
        std::cout<<'[';
        for(int rgb=0;rgb<3;++rgb) {
            if(rgb) std::cout<<',';
            if(duration>=25000) { display[col][row][rgb]=unsigned(std::min(uint64_t(255),exposure[col][row][rgb]*255*columns/duration)); exposure[col][row][rgb]=0; }
            std::cout<<display[col][row][rgb];
        }
        std::cout<<']';
    }
    std::cout<<"],\"sensor_reads\":"<<adc_reads<<",\"sensor_selects\":"<<sensor_selects<<",\"led_writes\":"<<led_writes<<"}\n"<<std::flush;
    midi.erase(std::remove_if(midi.begin(),midi.end(),[](auto b){return b.at<=now;}),midi.end());
    if(duration>=25000) frame_since=now;
}
}
SerialPort Serial;
SpiBus SPI;
uint32_t micros() { return uint32_t(++sim::now); }
uint32_t millis() { return uint32_t(++sim::now/1000); }
void delay(uint32_t ms) { sim::now+=uint64_t(ms)*1000; }
void delayMicroseconds(uint32_t us) { sim::now+=us; }
void pinMode(int pin,int mode) { if(pin<0||pin>=64) throw std::runtime_error("unsupported pin"); sim::modes[pin]=mode; }
void digitalWrite(int pin,int value) { if(pin<0||pin>=64 || sim::modes[pin]!=OUTPUT) throw std::runtime_error("invalid pin write"); sim::led_integrate(); sim::pins[pin]=value; ++sim::now; }
int digitalRead(int pin) { ++sim::now; if(pin==38) return sim::model==200?HIGH:LOW; if(pin==33||pin==34) return sim::pins[pin]; throw std::runtime_error("unsupported pin read"); }
int analogRead(int pin) { if(pin!=0) throw std::runtime_error("unsupported analog pin"); ++sim::now; return 1; }
void randomSeed(uint32_t seed) { if(seed!=0) sim::seed=seed; }
// Matches the pinned ARM newlib rand object's 64-bit recurrence and high-31-bit result.
int32_t random(int32_t bound) { if(bound<=0) return 0; sim::seed=sim::seed*UINT64_C(6364136223846793005)+1; return int32_t(((sim::seed>>32)&0x7fffffff)%uint32_t(bound)); }
int32_t random(int32_t low,int32_t high) { return high<=low?low:low+random(high-low); }
int32_t map(int32_t value,int32_t in_min,int32_t in_max,int32_t out_min,int32_t out_max) { if(in_max==in_min) throw std::runtime_error("map division by zero"); return (value-in_min)*(out_max-out_min)/(in_max-in_min)+out_min; }
void SerialPort::begin(int baud) { if(baud!=31250&&baud!=115200) throw std::runtime_error("unsupported baud"); sim::baud=baud; }
void SerialPort::flush() { sim::now=std::max(sim::now,sim::tx_ready); }
int SerialPort::available() { ++sim::now; return int(sim::rx.size()); }
int SerialPort::availableForWrite() { ++sim::now; const auto occupied=(sim::tx_ready>sim::now)?(sim::tx_ready-sim::now)*sim::baud/10000000:0; return int(64-std::min(uint64_t(64),occupied)); }
int SerialPort::read() { ++sim::now; if(sim::rx.empty()) return -1; auto value=sim::rx.front(); sim::rx.pop_front(); return value; }
size_t SerialPort::write(byte value) {
    if(sim::midi.size()==32768) throw std::runtime_error("MIDI trace capacity");
    if(availableForWrite()==0) sim::now=std::max(sim::now,sim::tx_ready-63*uint64_t(10000000/sim::baud));
    sim::tx_ready=std::max(sim::now,sim::tx_ready)+uint64_t(10000000/sim::baud);
    if(sim::pins[35]==LOW) sim::midi.push_back({sim::tx_ready,value});
    return 1;
}
size_t SerialPort::write(const byte* data,size_t length) { for(size_t i=0;i<length;++i) write(data[i]); return length; }
size_t SerialPort::write(const char* text) { return write(reinterpret_cast<const byte*>(text),std::strlen(text)); }
static void check_spi(int port) { if(port!=10&&port!=4&&port!=52) throw std::runtime_error("unsupported SPI select"); }
void SpiBus::begin(int port) { check_spi(port); }
void SpiBus::setDataMode(int port,int mode) { check_spi(port); if(mode!=0) throw std::runtime_error("unsupported SPI mode"); }
void SpiBus::setClockDivider(int port,int divider) { check_spi(port); if(divider!=4) throw std::runtime_error("unsupported SPI clock"); }
byte SpiBus::transfer(int port,byte data,int continued) {
    check_spi(port); ++sim::now;
    if(port==52) {
        if(!sim::adc_second) sim::adc_value=sim::sensor_read()<<2;
        byte result=sim::adc_second?byte(sim::adc_value):byte(sim::adc_value>>8);
        sim::adc_second=continued!=0; return result;
    }
    auto& bytes=port==4?sim::sensor_bytes:sim::led_bytes;
    bytes.push_back(data);
    if(bytes.size()>size_t(port==4?2:4)) throw std::runtime_error("SPI transaction overflow");
    if(continued) return 0;
    if(port==4) {
        if(bytes.size()!=2) throw std::runtime_error("sensor transaction length");
        sim::sensor_col=bytes[1]&31; sim::sensor_row=bytes[0]&7;
        if(sim::sensor_col >= (sim::model==200?26:17)) throw std::runtime_error("sensor column outside model");
        int code=bytes[0]&0x78; sim::axis=code==0x50?0:code==0x18?1:code==0x20?2:-1;
        if(sim::axis<0) throw std::runtime_error("sensor switch code");
        ++sim::sensor_selects;
    } else {
        if(bytes.size()!=4) throw std::runtime_error("LED transaction length");
        sim::led_integrate(); sim::led_col=((~bytes[0])>>2)&31;
        if(sim::led_col>=26) throw std::runtime_error("LED column");
        sim::led_bits={bytes[3],bytes[2],bytes[1]}; ++sim::led_writes;
    }
    bytes.clear(); return 0;
}
byte DueFlashStorage::read(uint32_t address) { if(address>=sim::flash.size()) throw std::runtime_error("flash read bounds"); return sim::flash[address]; }
byte* DueFlashStorage::readAddress(uint32_t address) { if(address>=sim::flash.size()) throw std::runtime_error("flash address bounds"); return sim::flash.data()+address; }
bool DueFlashStorage::write(uint32_t address,byte value) { return write(address,&value,1); }
bool DueFlashStorage::write(uint32_t address,byte* data,uint32_t length) { if(uint64_t(address)+length>sim::flash.size()) throw std::runtime_error("flash write bounds"); std::memcpy(sim::flash.data()+address,data,length); sim::now+=100; return true; }
