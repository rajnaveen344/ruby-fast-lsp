# frozen_string_literal: true

module RbConfig
  # A Hash with the bounds of numeric types available to the \C compiler
  # used to build Ruby. To access this constant, first run
  # <code>require 'rbconfig/sizeof'</code>.
  #
  #    require 'rbconfig/sizeof'
  #    RUBY_PLATFORM # => "x64-mingw-ucrt"
  #    RbConfig::LIMITS.fetch_values('FIXNUM_MAX', 'LONG_MAX')
  #    # => [1073741823, 2147483647]
  LIMITS = _
  # A Hash with the byte size of \C types available to the compiler
  # used to build Ruby. To access this constant, first run
  # <code>require 'rbconfig/sizeof'</code>.
  #
  #    require 'rbconfig/sizeof'
  #    RUBY_PLATFORM                                  # => "x64-mingw-ucrt"
  #    RbConfig::SIZEOF.fetch_values('long', 'void*') # => [4, 8]
  SIZEOF = _
end
