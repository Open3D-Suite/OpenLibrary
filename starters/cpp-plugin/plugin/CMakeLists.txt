# SPDX-License-Identifier: GPL-3.0-or-later
#
# OpenExample.gup: the plug-in for 3ds Max 2027. Built only when OPENEXAMPLE_BUILD_PLUGIN is ON, because it
# needs the 3ds Max SDK (OPENEXAMPLE_MAXSDK: the SDK's "maxsdk" folder) and a release build: 3ds Max loads
# plug-ins built against the release C runtime only. Its extension says what kind of plug-in it is: .gup
# for a global utility plug-in like this one, .dlo for an object, .dlm for a modifier...
enable_language(RC)

# The SDK builds its plug-ins with /EHa; take the default /EHsc out of this folder so that the compiler
# does not warn about one overriding the other.
string(REPLACE "/EHsc" "" CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS}")

if(NOT EXISTS "${OPENEXAMPLE_MAXSDK}/include/max.h")
    message(FATAL_ERROR "3ds Max SDK not found at OPENEXAMPLE_MAXSDK='${OPENEXAMPLE_MAXSDK}'. "
                        "Pass -DOPENEXAMPLE_MAXSDK=<path to the SDK's maxsdk folder>.")
endif()

# The version resource: the file properties of the plug-in carry the project version.
configure_file(version.rc.in "${CMAKE_CURRENT_BINARY_DIR}/version.rc" @ONLY)

add_library(OpenExample SHARED
    plugin.cpp
    OpenExample.def
    "${CMAKE_CURRENT_BINARY_DIR}/version.rc"
)

set_target_properties(OpenExample PROPERTIES
    PREFIX ""
    SUFFIX ".gup"
    MSVC_RUNTIME_LIBRARY "MultiThreadedDLL"
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/plugin"
    LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/plugin")

# The SDK's headers are not ours: no warnings from them.
target_include_directories(OpenExample SYSTEM PRIVATE "${OPENEXAMPLE_MAXSDK}/include")
target_link_directories(OpenExample PRIVATE "${OPENEXAMPLE_MAXSDK}/lib/x64/Release")

# The definitions and switches of the SDK's own property sheets (3dsmax.common.tools.settings.props).
target_compile_definitions(OpenExample PRIVATE
    UNICODE _UNICODE
    WIN32_LEAN_AND_MEAN NOMINMAX _ADESK_3DSMAX_WINDOWS_
    WINVER=0x0601 _WIN32_WINNT=0x0601 _WIN32_WINDOWS=0x0601 _WIN32_IE=0x0800
    _WINDOWS _USRDLL
    _CRT_SECURE_NO_DEPRECATE _CRT_NONSTDC_NO_DEPRECATE _SCL_SECURE_NO_DEPRECATE
    ISOLATION_AWARE_ENABLED=1
    MODULE_NAME=OpenExample.gup
    OPENEXAMPLE_VERSION="${PROJECT_VERSION}")
target_compile_options(OpenExample PRIVATE
    /GR /EHa /permissive- /utf-8 /Zc:__cplusplus /Zc:lambda /Zc:inline
    /external:anglebrackets /external:W0
    /W4 /wd4100)
if(OPENEXAMPLE_WARNINGS_AS_ERRORS)
    target_compile_options(OpenExample PRIVATE /WX)
endif()

target_link_libraries(OpenExample PRIVATE openexample_core core.lib gup.lib maxutil.lib paramblk2.lib)
