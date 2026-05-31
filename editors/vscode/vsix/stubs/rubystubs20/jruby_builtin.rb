# frozen_string_literal: true

# Require Ruby core built-in
require "#{File.dirname(__FILE__)}/builtin"

# Require main jruby class
#
# javasupport/core-ext adds java methods for core Ruby classes (for example to_java to Array)
# These methods are added automatically, it`s ok!
#
# java adds ruby like methods to Java classes (for example each to each to java.util.Map)
# These methods are not added automatically, we should do it manually!!!
#
# We should manually include JavaProxyMethods to each JavaProxyClass!!!
require '1.8/builtin/javasupport'
require 'builtin/javasupport' # JRuby 1.1.5

# Require JRuby extentions for java classes
require "#{File.dirname(__FILE__)}/jruby_extensions"
