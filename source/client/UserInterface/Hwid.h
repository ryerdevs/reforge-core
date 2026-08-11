#pragma once

// F2b (ADR-0007): 16-byte hardware identifier appended to the AUTH LOGIN3
// packet (TPacketCGLogin3.hwid). The CHANNEL does not send it (see
// SendLoginPacket/SendLoginPacketNew in PythonNetworkStreamPhaseLogin.cpp).
//
// Sources, in order (first success wins):
//  1. HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid (REG_SZ, 32 hex chars,
//     dashes tolerated) -> hex-decoded to 16 bytes.
//  2. C:\ volume serial via GetVolumeInformationA (4 bytes) + 12 zero bytes
//     (deterministic padding).
//  3. 16 zero bytes.
//
// advapi32.lib is needed for the registry calls (RegOpenKeyExA /
// RegQueryValueExA); declared here so no project file change is required
// (the same pattern is already used in EterLib/IME.cpp).

#pragma comment(lib, "advapi32.lib")

#include <windows.h>

inline void GetMachineHwid(char* hwid) // out: exactly 16 bytes
{
    bool bOk = false;

    char szGuid[64] = { 0 };
    HKEY hKey = nullptr;
    // KEY_WOW64_64KEY: this is a 32-bit client; without it the read is
    // redirected to WOW6432Node (where MachineGuid usually does not exist).
    if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Cryptography", 0, KEY_READ | KEY_WOW64_64KEY, &hKey) == ERROR_SUCCESS)
    {
        DWORD dwType = 0;
        DWORD dwSize = sizeof(szGuid);
        if (RegQueryValueExA(hKey, "MachineGuid", nullptr, &dwType, (LPBYTE)szGuid, &dwSize) == ERROR_SUCCESS && dwType == REG_SZ)
        {
            // Hex-decode, skipping non-hex chars (e.g. dashes); stop at 16 bytes.
            int iByte = 0;
            int iNibble = 0;
            for (const char* p = szGuid; *p && iByte < 16; ++p)
            {
                BYTE bVal = 0xFF;
                if (*p >= '0' && *p <= '9') bVal = (BYTE)(*p - '0');
                else if (*p >= 'a' && *p <= 'f') bVal = (BYTE)(*p - 'a' + 10);
                else if (*p >= 'A' && *p <= 'F') bVal = (BYTE)(*p - 'A' + 10);
                if (bVal == 0xFF)
                    continue;
                if (iNibble == 0)
                    hwid[iByte] = (char)(bVal << 4);
                else
                    hwid[iByte++] |= (char)bVal;
                iNibble ^= 1;
            }
            bOk = (iByte == 16);
        }
        RegCloseKey(hKey);
    }

    if (!bOk)
    {
        DWORD dwSerial = 0;
        if (GetVolumeInformationA("C:\\", nullptr, 0, &dwSerial, nullptr, nullptr, nullptr, 0))
        {
            memcpy(hwid, &dwSerial, 4);
            memset(hwid + 4, 0, 12);
            bOk = true;
        }
    }

    if (!bOk)
        memset(hwid, 0, 16);
}
