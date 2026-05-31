# frozen_string_literal: true

# The <code>GC</code> module provides an interface to Ruby's mark and
# sweep garbage collection mechanism. Some of the underlying methods
# are also available via the <code>ObjectSpace</code> module.
module GC
  # Disables garbage collection, returning <code>true</code> if garbage
  # collection was already disabled.
  #
  #    GC.disable   #=> false
  #    GC.disable   #=> true
  def self.disable; end

  # Enables garbage collection, returning <code>true</code> if garbage
  # collection was previously disabled.
  #
  #    GC.disable   #=> false
  #    GC.enable    #=> true
  #    GC.enable    #=> false
  def self.enable; end

  # Initiates garbage collection, unless manually disabled.
  def self.start; end

  # returns current status of GC stress mode.
  def self.stress; end

  # updates GC stress mode.
  #
  # When GC.stress = true, GC is invoked for all GC opportunity:
  # all memory and object allocation.
  #
  # Since it makes Ruby very slow, it is only for debugging.
  def self.stress=(bool) end

  # Initiates garbage collection, unless manually disabled.
  def garbage_collect; end
end
