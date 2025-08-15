# frozen_string_literal: true

class JavaUtilMap
  include Enumerable

  def [](key) end

  def []=(key, val) end

  def each(&block) end
end

class JavaLangComparable
  include Comparable

  def <=>(other) end
end

class JavaLangObject
  def field_accessor(*args) end

  def field_reader(*args) end

  def field_writer(*args) end
end

class JavaUtilCollection
  include Enumerable

  def +(other) end

  def -(other) end

  def <<(a) end

  def each(&block) end

  def join(*args) end

  def length; end
end

class JavaUtilEnumeration
  include Enumerable

  def each; end
end

class JavaUtilIterator
  include Enumerable

  def each; end
end

class JavaUtilList
  def [](ix) end

  def []=(ix, val) end

  def sort; end

  def sort!; end

  def _wrap_yield(*args) end
end

class JavaLangRunnable
  def to_proc; end
end

class String
  def self.from_java_bytes(bytes) end

  def to_java_bytes; end
end

class Array
  def to_java(arg = nil) end
end

class Hash
  def to_java(arg = nil) end
end
