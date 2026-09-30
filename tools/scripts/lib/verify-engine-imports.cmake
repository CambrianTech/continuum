# Run only by the engine build owner, after staging the application DLL namespace.
# System/driver dependencies are intentionally not copied or hashed as app inputs.
cmake_minimum_required(VERSION 3.21)
# normalize resolved paths before the exclusion regexes match them (CMake 4.x policy)
if(POLICY CMP0207)
  cmake_policy(SET CMP0207 NEW)
endif()
if(NOT IS_ABSOLUTE "${ENGINE_DIR}" OR NOT IS_ABSOLUTE "${SYSTEM_DIR}")
  message(FATAL_ERROR "Engine dependency roots must be absolute")
endif()
file(GLOB app_dlls "${ENGINE_DIR}/*.dll")
# The platform's own DLLs are resolved (an app import landing there is allowed) but NOT
# recursed: their imports are the OS's business, and on some installs they name feature-on-demand
# DLLs that are simply absent (AzureAttestManager, HvsiFileTrust, PdmUtilities, wpaxholder via
# shell32 on the 5090, 2026-09-27), which failed every engine build. Windows paths compare
# case-insensitively, so the regex is built one [Xx] class per letter.
string(REPLACE "\\" "/" system_path "${SYSTEM_DIR}")
set(system_regex "")
string(LENGTH "${system_path}" system_len)
math(EXPR system_last "${system_len} - 1")
foreach(i RANGE 0 ${system_last})
  string(SUBSTRING "${system_path}" ${i} 1 ch)
  string(TOUPPER "${ch}" up)
  string(TOLOWER "${ch}" low)
  if(NOT up STREQUAL low)
    string(APPEND system_regex "[${up}${low}]")
  elseif(ch STREQUAL ".")
    string(APPEND system_regex "[.]")
  elseif(ch STREQUAL "/")
    # resolved paths mix separators (C:\windows\system32/nvcuda.dll): match either
    string(APPEND system_regex "[/\\\\]")
  else()
    string(APPEND system_regex "${ch}")
  endif()
endforeach()
set(CMAKE_GET_RUNTIME_DEPENDENCIES_PLATFORM "windows+pe")
set(CMAKE_GET_RUNTIME_DEPENDENCIES_TOOL "dumpbin")
file(GET_RUNTIME_DEPENDENCIES
  EXECUTABLES "${ENGINE_DIR}/llama-server.exe"
  LIBRARIES ${app_dlls}
  DIRECTORIES "${ENGINE_DIR}" "${SYSTEM_DIR}"
  PRE_EXCLUDE_REGEXES "[Aa][Pp][Ii]-[Mm][Ss]-.*" "[Ee][Xx][Tt]-[Mm][Ss]-.*"
  POST_EXCLUDE_REGEXES "^${system_regex}[/\\\\]"
  RESOLVED_DEPENDENCIES_VAR resolved
  UNRESOLVED_DEPENDENCIES_VAR unresolved
  CONFLICTING_DEPENDENCIES_PREFIX conflicting)
if(unresolved OR conflicting_FILENAMES)
  message(FATAL_ERROR "Unresolved/conflicting engine imports: ${unresolved};${conflicting_FILENAMES}")
endif()
foreach(dependency IN LISTS resolved)
  cmake_path(GET dependency PARENT_PATH parent)
  string(TOLOWER "${parent}" parent)
  string(TOLOWER "${ENGINE_DIR}" app)
  string(TOLOWER "${SYSTEM_DIR}" system)
  if(NOT parent STREQUAL app AND NOT parent STREQUAL system)
    message(FATAL_ERROR "Engine imports outside app/platform roots: ${dependency}")
  endif()
endforeach()
