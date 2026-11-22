# SPDX-License-Identifier: GPL-3.0-or-later
#
# OpenExample: <what it does>, a plug-in for 3ds Max. This builds the part that needs no 3ds Max (a
# library and its tests) and, when asked, since it needs the 3ds Max SDK, the plug-in itself.
cmake_minimum_required(VERSION 3.24)
project(OpenExample VERSION 0.1.0 LANGUAGES CXX)

option(OPENEXAMPLE_BUILD_TESTS "Build the tests of the library" ON)
option(OPENEXAMPLE_BUILD_PLUGIN "Build the 3ds Max plug-in (needs the 3ds Max SDK: set OPENEXAMPLE_MAXSDK)" OFF)
option(OPENEXAMPLE_WARNINGS_AS_ERRORS "Treat compiler warnings as errors" ON)
set(OPENEXAMPLE_MAXSDK "$ENV{ADSK_3DSMAX_SDK_2027}" CACHE PATH "The maxsdk folder of the 3ds Max 2027 SDK (it holds include and lib)")

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Every program in one folder of the build tree.
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")

# Compile options shared by every target of ours.
function(openexample_target_options target)
    if(MSVC)
        target_compile_options(${target} PRIVATE /W4 /permissive- /utf-8)
        if(OPENEXAMPLE_WARNINGS_AS_ERRORS)
            target_compile_options(${target} PRIVATE /WX)
        endif()
    else()
        target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic)
        if(OPENEXAMPLE_WARNINGS_AS_ERRORS)
            target_compile_options(${target} PRIVATE -Werror)
        endif()
    endif()
endfunction()

# The part that needs no 3ds Max: where the plug-in keeps its files, from what every Open3D app shares
# (src/open3d_settings.cpp, a copy of a common file).
add_library(openexample_core STATIC
    src/folders.h
    src/folders.cpp
    src/open3d_settings.h
    src/open3d_settings.cpp
)
target_include_directories(openexample_core PUBLIC src)
openexample_target_options(openexample_core)

if(OPENEXAMPLE_BUILD_TESTS)
    enable_testing()
    add_executable(openexample_tests tests/test_folders.cpp)
    target_link_libraries(openexample_tests PRIVATE openexample_core)
    openexample_target_options(openexample_tests)
    add_test(NAME openexample_tests COMMAND openexample_tests)
endif()

# 3ds Max only loads plug-ins built against the release C runtime: no plug-in in a debug build.
if(OPENEXAMPLE_BUILD_PLUGIN)
    if(MSVC AND NOT CMAKE_BUILD_TYPE STREQUAL "Debug")
        add_subdirectory(plugin)
    else()
        message(FATAL_ERROR "OPENEXAMPLE_BUILD_PLUGIN needs MSVC and a release build.")
    endif()
endif()
