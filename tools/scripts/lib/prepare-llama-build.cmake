# One shared cache is reused by both installers and by every checkout. CMake
# metadata belongs to a source directory; object/build output is only a cache.
cmake_minimum_required(VERSION 3.14)

if(NOT DEFINED SOURCE_DIR OR NOT DEFINED BUILD_DIR)
    message(FATAL_ERROR "llama-server cache preparation requires SOURCE_DIR and BUILD_DIR")
endif()
get_filename_component(source "${SOURCE_DIR}" REALPATH)
get_filename_component(build "${BUILD_DIR}" REALPATH)
get_filename_component(build_name "${build}" NAME)
if(NOT IS_DIRECTORY "${source}" OR NOT EXISTS "${source}/CMakeLists.txt")
    message(FATAL_ERROR "llama-server source is not a CMake project: ${source}")
endif()
if(NOT IS_DIRECTORY "${build}" OR NOT build_name STREQUAL "llama-server-build" OR build STREQUAL source)
    message(FATAL_ERROR "refusing to prepare an unexpected llama-server build directory: ${build}")
endif()

# These are the only paths this helper may remove, both direct children of the
# canonical build directory. Refuse redirected metadata instead of following it.
set(cache "${build}/CMakeCache.txt")
set(metadata "${build}/CMakeFiles")
if(IS_SYMLINK "${cache}" OR IS_SYMLINK "${metadata}")
    message(FATAL_ERROR "llama-server cache metadata is a symlink; refusing to reset ${build}")
endif()
if(NOT EXISTS "${cache}")
    return()
endif()

# A read failure is a CMake fatal error. A readable cache without one explicit
# owner is also an error, not permission to erase potentially useful evidence.
file(STRINGS "${cache}" owners REGEX "^CMAKE_HOME_DIRECTORY:INTERNAL=")
list(LENGTH owners owner_count)
if(NOT owner_count EQUAL 1)
    message(FATAL_ERROR "cannot identify the source owner of ${cache}; cache left unchanged")
endif()
string(REGEX REPLACE "^CMAKE_HOME_DIRECTORY:INTERNAL=" "" owner "${owners}")
if(owner STREQUAL "" OR NOT IS_ABSOLUTE "${owner}")
    message(FATAL_ERROR "invalid source owner in ${cache}; cache left unchanged")
endif()
get_filename_component(owner "${owner}" REALPATH)
set(source_key "${source}")
set(owner_key "${owner}")
if(WIN32)
    string(TOLOWER "${source_key}" source_key)
    string(TOLOWER "${owner_key}" owner_key)
endif()
if(source_key STREQUAL owner_key)
    return()
endif()

message(STATUS "llama-server source changed: ${owner} -> ${source}; configuring fresh metadata in ${build}")
# Equivalent to --fresh without raising the supported CMake version floor.
# Preserve other cache contents; never remove the build directory itself.
file(REMOVE "${cache}")
file(REMOVE_RECURSE "${metadata}")
if(EXISTS "${cache}" OR EXISTS "${metadata}")
    message(FATAL_ERROR "could not reset llama-server CMake metadata in ${build}; configure aborted")
endif()
