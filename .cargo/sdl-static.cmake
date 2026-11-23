set(SDL_FORCE_STATIC_VCRT ON CACHE BOOL "Use the same static C runtime as Rust" FORCE)
set(CMAKE_C_FLAGS_INIT "-D__FLTUSED__")
