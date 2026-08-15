target("WeaselUI")
  set_kind("static")
  add_files("./*.cpp")
  add_files("./miniz.c")
  add_cxflags("/openmp")  -- Enable OpenMP for parallel processing
