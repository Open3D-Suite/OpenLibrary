// SPDX-License-Identifier: GPL-3.0-or-later
//
// Where OpenExample keeps its files, from what every Open3D app shares (open3d_settings.h). Plain C++ with no
// 3ds Max header, so that it builds and is tested without the 3ds Max SDK. The plug-in's interface is in
// English only: open3d_settings gives it its folders, never a language.

#pragma once

#include "open3d_settings.h"

#include <filesystem>
#include <optional>
#include <string>

namespace openexample
{

// The app's name, as the Open3D launcher shows it and as its folders are called.
inline constexpr const wchar_t* kApp = L"OpenExample";

// The folder of the app's settings and the folder of everything else it writes, one per line. A missing
// folder is said. Nothing is created.
std::wstring describe(const std::optional<std::filesystem::path>& settings,
                      const std::optional<std::filesystem::path>& data);

// describe() for this computer: %APPDATA%\Open3D\OpenExample and %LOCALAPPDATA%\Open3D\OpenExample, or
// OpenExample in the folders that OPEN3D_SETTINGS_DIR and OPEN3D_DATA_DIR name.
std::wstring describe_here();

} // namespace openexample
