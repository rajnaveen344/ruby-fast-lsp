# frozen_string_literal: true

# Runtime APIs shared by every supported JRuby series. MRI-compatible core APIs
# remain owned by the selected Ruby compatibility baseline.

JRUBY_VERSION = _
JRUBY_REVISION = _

module Java
  def self.import(package_name, &block) end
end

module JavaUtilities
  def self.get_java_class(name) end

  def self.get_proxy_class(java_class) end

  def self.get_package_module(package) end

  def self.include_package(package) end

  def self.java_alias(new_id, old_id) end
end

class Object
  def java_kind_of?(other) end

  private

  def java_import(*import_classes) end
end

class Module
  private

  def import(package_or_class, &block) end

  def include_package(package) end

  def java_alias(new_id, old_id) end
end

module Kernel
  def java_package(*args) end

  def to_java(*args) end

  def java_signature(*args) end

  def java_implements(*args) end

  def java_annotation(*args) end

  def java_field(*args) end

  # @unavailable JRuby does not implement process forking on the JVM.
  def fork(*args) end

  # @unavailable JRuby does not implement process forking on the JVM.
  def self.fork(*args) end
end

module Process
  # @unavailable JRuby does not implement process forking on the JVM.
  def self.fork(*args) end
end

module ObjectSpace
  # @absent JRuby does not expose MRI's ObjectSpace.dump API.
  def self.dump(object, options = {}) end

  # @absent JRuby does not expose MRI's ObjectSpace.dump_all API.
  def self.dump_all(options = {}) end
end

module JavaProxyMethods
  def java_class() end

  def java_object() end

  def java_object=(object) end

  def synchronized() end

  def to_java_object() end
end

class JavaProxy
  include JavaProxyMethods

  def java_send(*args) end

  def java_method(*args) end
end

class ConcreteJavaProxy < JavaProxy
end

class ArrayJavaProxy < ConcreteJavaProxy
  include Enumerable
end

class Class
  def java_class() end

  def become_java!(*args) end
end

class String
  def self.from_java_bytes(bytes) end

  def to_java_bytes() end

  def to_java_string() end
end
