// SPDX-License-Identifier: GPL-3.0-or-later
//
// The tests of what needs no 3ds Max. No framework: a check that fails says where, and the program ends
// with 1 when any did. build.ps1 runs it after every build.

#include "folders.h"

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <string>
#include <system_error>

namespace
{

int g_failed = 0;

void check(bool passed, const char* what, int line)
{
    if (!passed)
    {
        std::printf("FAILED, line %d: %s\n", line, what);
        ++g_failed;
    }
}

#define CHECK(condition) check((condition), #condition, __LINE__)

void setVariable(const wchar_t* name, const std::filesystem::path& value)
{
#ifdef _WIN32
    _wputenv_s(name, value.c_str());
#else
    setenv(std::filesystem::path(name).string().c_str(), value.string().c_str(), 1);
#endif
}

void theTextNamesBothFolders()
{
    const std::filesystem::path settings = L"S:\\Open3D\\OpenExample";
    const std::filesystem::path data = L"D:\\Open3D\\OpenExample";
    CHECK(openexample::describe(settings, data) ==
          L"Settings: S:\\Open3D\\OpenExample\nOther files: D:\\Open3D\\OpenExample");
}

void aMissingFolderIsSaid()
{
    CHECK(openexample::describe(std::nullopt, std::nullopt) == L"Settings: (no folder)\nOther files: (no folder)");
}

// With the variables every Open3D app reads pointing at a temporary folder: the app's two folders, in
// English even when the shared language is Spanish (a 3ds Max plug-in is in English only), and nothing
// created.
void theAppsFoldersAreUsedInEnglish()
{
    std::error_code error;
    const std::filesystem::path root = std::filesystem::temp_directory_path() / L"openexample-tests";
    std::filesystem::remove_all(root, error);
    std::filesystem::create_directories(root / L"settings");
    std::ofstream(root / L"settings" / L"settings.json") << R"({"language": "es"})";
    setVariable(L"OPEN3D_SETTINGS_DIR", root / L"settings");
    setVariable(L"OPEN3D_DATA_DIR", root / L"data");

    const std::wstring expected = L"Settings: " + (root / L"settings" / L"OpenExample").wstring() + L"\nOther files: " +
                                  (root / L"data" / L"OpenExample").wstring();
    CHECK(openexample::describe_here() == expected);
    CHECK(!std::filesystem::exists(root / L"settings" / L"OpenExample"));
    CHECK(!std::filesystem::exists(root / L"data"));
    std::filesystem::remove_all(root, error);
}

} // namespace

int main()
{
    theTextNamesBothFolders();
    aMissingFolderIsSaid();
    theAppsFoldersAreUsedInEnglish();
    if (g_failed != 0)
    {
        std::printf("%d checks FAILED\n", g_failed);
        return EXIT_FAILURE;
    }
    std::printf("All checks passed\n");
    return EXIT_SUCCESS;
}
