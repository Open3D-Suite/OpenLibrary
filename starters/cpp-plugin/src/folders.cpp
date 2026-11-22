// SPDX-License-Identifier: GPL-3.0-or-later
//
// Where OpenExample keeps its files (see folders.h).

#include "folders.h"

namespace openexample
{

std::wstring describe(const std::optional<std::filesystem::path>& settings,
                      const std::optional<std::filesystem::path>& data)
{
    const auto shown = [](const std::optional<std::filesystem::path>& folder) {
        return folder ? folder->wstring() : std::wstring(L"(no folder)");
    };
    return L"Settings: " + shown(settings) + L"\nOther files: " + shown(data);
}

std::wstring describe_here()
{
    return describe(open3d_settings::app_settings_dir(kApp), open3d_settings::app_data_dir(kApp));
}

} // namespace openexample
