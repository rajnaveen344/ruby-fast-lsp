# frozen_string_literal: true

class DBM
  include Enumerable

  NEWDB = _
  # flags for dbm_open()
  READER = _
  VERSION = _
  WRCREAT = _
  WRITER = _

  def self.open(*args) end

  def initialize(p1, p2 = v2, p3 = v3) end

  def [](p1) end

  def []=(p1, p2) end
  alias store []=

  def clear; end

  def close; end

  def closed?; end

  def delete(p1) end

  def delete_if; end
  alias reject! delete_if

  def each; end
  alias each_pair each

  def each_key; end

  def each_value; end

  def empty?; end

  def fetch(p1, p2 = v2) end

  def has_value?(p1) end
  alias value? has_value?

  def include?(p1) end
  alias has_key? include?
  alias member? include?
  alias key? include?

  def index(p1) end

  def indexes(*args) end
  alias indices indexes

  def invert; end

  def keys; end

  def length; end
  alias size length

  def reject; end

  def replace(p1) end

  def select(*args) end

  def shift; end

  def to_a; end

  def to_hash; end

  def update(p1) end

  def values; end

  def values_at(*args) end
end
