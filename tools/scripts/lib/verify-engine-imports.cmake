# Run only by the engine build owner, after staging the application DLL namespace.
# System/driver dependencies are intentionally not copied or hashed as app inputs.
cmake_minimum_required(VERSION 3.21)
if(NOT IS_ABSOLUTE "${ENGINE_DIR}" OR NOT IS_ABSOLUTE "${SYSTEM_DIR}")
  message(FATAL_ERROR "Engine dependency roots must be absolute")
endif()
file(GLOB app_dlls "${ENGINE_DIR}/*.dll")
set(CMAKE_GET_RUNTIME_DEPENDENCIES_PLATFORM "windows+pe")
set(CMAKE_GET_RUNTIME_DEPENDENCIES_TOOL "dumpbin")
file(GET_RUNTIME_DEPENDENCIES
  EXECUTABLES "${ENGINE_DIR}/llama-server.exe"
  LIBRARIES ${app_dlls}
  DIRECTORIES "${ENGINE_DIR}" "${SYSTEM_DIR}"
  PRE_EXCLUDE_REGEXES "[Aa][Pp][Ii]-[Mm][Ss]-.*" "[Ee][Xx][Tt]-[Mm][Ss]-.*"
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
