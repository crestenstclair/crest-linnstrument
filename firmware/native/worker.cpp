#include <fstream>
#include <sstream>
#include "sim.hpp"
#include "sketch.cpp"
#undef min
#undef max
#undef constrain

static_assert(sizeof(int)==4 && sizeof(short)==2 && sizeof(bool)==1, "SAM scalar widths");
static_assert(sizeof(uint32_t)==4, "SAM time width");

static_assert(sizeof(Configuration)==11720, "ARM Configuration layout");
static_assert(sizeof(DeviceSettings)==2956, "ARM DeviceSettings layout");
static_assert(sizeof(GlobalSettings)==168, "ARM GlobalSettings layout");
static_assert(sizeof(SplitSettings)==98, "ARM SplitSettings layout");
static_assert(sizeof(PresetSettings)==364, "ARM PresetSettings layout");
static_assert(sizeof(SequencerProject)==6214, "ARM SequencerProject layout");
static_assert(sizeof(TouchInfo)==52, "ARM TouchInfo layout");
static_assert(sizeof(CalibrationSample)==4 && sizeof(CalibrationY)==7, "ARM calibration layout");
#include "arm-layout.inc"

void sim::surface_metadata(std::ostream& out) {
    out << "{\"normal\":" << (displayMode == displayNormal ? "true" : "false")
        << ",\"user_mode\":" << (userFirmwareActive ? "true" : "false") << ",\"notes\":[";
    for (int index=0; index<NUMCOLS*8; ++index) {
        const byte col=index/8, row=index%8;
        if(col || row) out << ',';
        const byte split=getSplitOf(col);
        const auto& settings=Split[split];
        const short note=transposedNote(split,col,row);
        const bool ordinary=col && displayMode==displayNormal && !userFirmwareActive &&
            !settings.ccFaders && !settings.strum && !settings.sequencer &&
            (row!=0 || settings.lowRowMode==lowRowNormal) && note>=0 && note<=127;
        if(ordinary) out << note; else out << "null";
    }
    out << "],\"highlights\":[";
    for (int index=0; index<NUMCOLS*8; ++index) {
        const byte col=index/8, row=index%8;
        if(col || row) out << ',';
        const byte split=getSplitOf(col);
        const short displayed=getNoteNumber(split,col,row)+Split[split].transposeOctave;
        const int mask=1 << std::abs(displayed%12);
        const char* role=customLedPatternActive ? "custom" :
            (Global.accentNotes[Global.activeNotes]&mask) ? "accent" :
            (Global.mainNotes[Global.activeNotes]&mask) ? "scale" : "unmarked";
        out << '"' << role << '"';
    }
    out << "]}";
}

static void save_flash(const std::string& path) {
    std::ofstream out(path,std::ios::binary);
    out.write(reinterpret_cast<const char*>(sim::flash.data()),sim::flash.size());
    if(!out) throw std::runtime_error("flash save failed");
}
static void load_flash(const std::string& path) {
    std::ifstream in(path,std::ios::binary);
    in.read(reinterpret_cast<char*>(sim::flash.data()),sim::flash.size());
    if(in.gcount()!=std::streamsize(sim::flash.size()) || in.peek()!=EOF) throw std::runtime_error("flash image size");
}
static void make_ideal_fixture(const std::string& path) {
    // Fixture preparation only: persist ideal upstream default calibration and
    // an explicitly selected MPE profile. Normal worker input has no such hook.
    setup();
    initializeCalibrationData();
    Device.calibrated=true;
    Device.calCrcCalculated=true;
    Device.calCrc=calculateCalibrationCRC();
    Device.serialMode=false;
    enableMpe(LEFT,1,15);
    Split[LEFT].pitchCorrectQuantize=false;
    Split[LEFT].pitchCorrectHold=pitchCorrectHoldOff;
    storeSettings();
    save_flash(path);
}
static void advance(uint64_t duration) {
    if(duration>1000000) throw std::runtime_error("advance limit is one second");
    uint64_t end=sim::now+duration;
    while(sim::now<end) { ++sim::now; loop(); }
}
static void command(const std::string& line) {
    std::istringstream in(line);
    std::string name; in>>name;
    if(name=="advance") { uint64_t duration; if(!(in>>duration)) throw std::runtime_error("advance argument"); advance(duration); }
    else if(name=="contact") { sim::Contact c; if(!(in>>c.id>>c.x>>c.y>>c.z)) throw std::runtime_error("contact arguments"); sim::contact(c); }
    else if(name=="release") { int id; if(!(in>>id)) throw std::runtime_error("release argument"); sim::release(id); }
    else if(name=="release_all") sim::contacts.clear();
    else if(name=="switch") { int number,pressed; if(!(in>>number>>pressed) || (pressed!=0 && pressed!=1)) throw std::runtime_error("switch arguments"); sim::foot_switch(number,pressed!=0); }
    else if(name=="midi") {
        int value; size_t added=0;
        while(in>>value) { if(value<0 || value>255 || sim::rx.size()==1024) throw std::runtime_error("MIDI FIFO capacity or byte range"); sim::rx.push_back(byte(value)); ++added; }
        if(!in.eof() || !added) throw std::runtime_error("MIDI arguments");
        in.clear();
    }
    else if(name=="save") { std::string path; if(!(in>>path)) throw std::runtime_error("save path"); save_flash(path); }
    else if(name!="snapshot") throw std::runtime_error("unknown worker command");
    std::string extra; if(in>>extra) throw std::runtime_error("trailing argument");
    sim::snapshot();
}
int main(int argc,char** argv) {
    try {
        if(argc!=3 && argc!=4 && argc!=5) throw std::runtime_error("usage: worker <128|200> <flash-image|erased> [--make-fixture | --start-us N]");
        if(argc==5) {
            if(std::string(argv[3])!="--start-us") throw std::runtime_error("unknown flag");
            sim::now=std::stoull(argv[4]);
            if(sim::now>UINT64_C(0xffffffff)) throw std::runtime_error("start time exceeds first firmware wrap");
        }
        sim::initialize(std::stoi(argv[1]));
        if(argc==4) {
            if(std::string(argv[3])!="--make-fixture") throw std::runtime_error("unknown flag");
            make_ideal_fixture(argv[2]); return 0;
        }
        if(std::string(argv[2])!="erased") load_flash(argv[2]);
        setup();
        advance(100000);
        sim::snapshot();
        std::array<char,65537> line{};
        while(std::cin.getline(line.data(),line.size())) command(line.data());
        if(!std::cin.eof()) throw std::runtime_error("command too large or input failure");
    } catch(const std::exception& error) { std::cerr<<"firmware worker: "<<error.what()<<'\n'; return 1; }
}
