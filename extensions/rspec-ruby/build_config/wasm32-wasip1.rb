MRuby::Build.new do |conf|
  conf.toolchain :gcc
  conf.gembox "stdlib"
end

MRuby::CrossBuild.new("wasm32-wasip1") do |conf|
  toolchain = ENV.fetch("WASI_SDK_PATH") do
    raise "WASI_SDK_PATH must point to a wasi-sdk installation"
  end

  conf.toolchain :clang

  conf.cc.command = "#{toolchain}/bin/clang"
  conf.cc.flags << "--target=wasm32-wasip1"
  conf.cc.defines << "MRB_USE_WASM_TRAP_EXCEPTION" if ENV["WASI_TRAP_EXCEPTIONS"] == "1"
  if ENV["WASI_ENABLE_SJLJ"] == "1"
    conf.cc.flags << "-fwasm-exceptions"
    conf.cc.flags << "-mllvm"
    conf.cc.flags << "-wasm-enable-sjlj"
  end
  conf.cc.flags << "-Oz"

  conf.cxx.command = "#{toolchain}/bin/clang++"
  conf.cxx.flags << "--target=wasm32-wasip1"
  conf.cxx.defines << "MRB_USE_WASM_TRAP_EXCEPTION" if ENV["WASI_TRAP_EXCEPTIONS"] == "1"
  conf.cxx.flags << "-fwasm-exceptions"
  conf.cxx.flags << "-Oz"

  conf.linker.command = "#{toolchain}/bin/clang"
  conf.linker.flags << "--target=wasm32-wasip1"
  if ENV["WASI_ENABLE_SJLJ"] == "1"
    conf.linker.flags << "-fwasm-exceptions"
    conf.linker.flags << "-mllvm"
    conf.linker.flags << "-wasm-enable-sjlj"
  end
  conf.linker.flags << "-Oz"

  conf.archiver.command = "#{toolchain}/bin/llvm-ar"

  conf.enable_cxx_exception if ENV["WASI_USE_CXX_EXCEPTION"] == "1"

  conf.gembox "stdlib"
end
