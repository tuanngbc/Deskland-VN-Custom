// Original calendario.h mapping, ICU construction and localized formatting.
// All UCalendar/UDateFormat objects close inside each read. Only the original
// trusted System32 ICU code reference lives until exit; no worker is started.
#include "upstream/calendario.h"
#include <algorithm>
#include <cstdint>
#include <cwchar>

struct CalendarMonth {
    std::int32_t calendar, days, first, today;
    wchar_t title[128], date[128];
};

extern "C" int tlauncher_calendar_read(int calendar, const wchar_t* locale,
                                      int offset, double instant, double today, CalendarMonth* out) noexcept {
    if (!out) return 0;
    *out = {};
    if (!locale || offset < -12000 || offset > 12000) return 0;
    // These are the application's supported UI locales, not arbitrary ICU
    // keywords. The original fixed-size locale builder remains bounded.
    if (wcscmp(locale,L"vi-VN") && wcscmp(locale,L"en-US") && wcscmp(locale,L"zh-CN")) return 0;
    try {
        const int id = calendar ? calendar : calendario::tipoUtente();
        const auto month = calendario::costruisci(id,locale,offset,instant);
        const auto current = calendario::costruisci(id,locale,0,today);
        if (!month.ok || !current.ok || month.titolo.size() >= 128 || current.dataLunga.size() >= 128) return 0;
        out->calendar=id; out->days=month.giorni; out->first=month.lunediPrimo;
        out->today=month.titolo==current.titolo ? current.oggi : 0;
        std::copy(month.titolo.begin(),month.titolo.end(),out->title);
        std::copy(current.dataLunga.begin(),current.dataLunga.end(),out->date);
        return 1;
    } catch (...) { *out={}; return 0; }
}
