target("WeaselUI")
  set_kind("static")
  add_files("./*.cpp")
  -- miniz.c is a C source; the workspace-wide /TP flag forces C++,
  -- which breaks C tentative definitions, so compile it as C explicitly.
  add_files("./miniz.c", {flags = "/TC"})
  add_cxflags("/openmp")  -- Enable OpenMP for parallel processing
