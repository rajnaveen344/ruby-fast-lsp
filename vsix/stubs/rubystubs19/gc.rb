# frozen_string_literal: true

# The <code>GC</code> module provides an interface to Ruby's mark and
# sweep garbage collection mechanism. Some of the underlying methods
# are also available via the ObjectSpace module.
#
# You may obtain information about the operation of the GC through
# GC::Profiler.
module GC
  # The number of times GC occurred.
  #
  # It returns the number of times GC occurred since the process started.
  def self.count; end

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

  # The allocated size by malloc().
  #
  # It returns the allocated size by malloc().
  def self.malloc_allocated_size; end

  # The number of allocated memory object by malloc().
  #
  # It returns the number of allocated memory object by malloc().
  def self.malloc_allocations; end

  # Initiates garbage collection, unless manually disabled.
  def self.start; end

  # Returns a Hash containing information about the GC.
  #
  # The hash includes information about internal statistics about GC such as:
  #
  #   {
  #     :count          => 18,
  #     :heap_used      => 77,
  #     :heap_length    => 77,
  #     :heap_increment => 0,
  #     :heap_live_num  => 23287,
  #     :heap_free_num  => 8115,
  #     :heap_final_num => 0,
  #   }
  #
  # The contents of the hash are implementation defined and may be changed in
  # the future.
  #
  # This method is only expected to work on C Ruby.
  def self.stat; end

  # returns current status of GC stress mode.
  def self.stress; end

  # Updates the GC stress mode.
  #
  # When stress mode is enabled the GC is invoked at every GC opportunity:
  # all memory and object allocations.
  #
  # Enabling stress mode makes Ruby very slow, it is only for debugging.
  def self.stress=(bool) end

  # Initiates garbage collection, unless manually disabled.
  def garbage_collect; end

  # The GC profiler provides access to information on GC runs including time,
  # length and object space size.
  #
  # Example:
  #
  #   GC::Profiler.enable
  #
  #   require 'rdoc/rdoc'
  #
  #   puts GC::Profiler.result
  #
  #   GC::Profiler.disable
  #
  # See also GC.count, GC.malloc_allocated_size and GC.malloc_allocations
  module Profiler
    # Clears the GC profiler data.
    def self.clear; end

    # Stops the GC profiler.
    def self.disable; end

    # Starts the GC profiler.
    def self.enable; end

    # The current status of GC profile mode.
    def self.enabled?; end

    # Writes the GC::Profiler#result to <tt>$stdout</tt> or the given IO object.
    def self.report(*several_variants) end

    # Returns a profile data report such as:
    #
    #   GC 1 invokes.
    #   Index    Invoke Time(sec)       Use Size(byte)     Total Size(byte)         Total Object                    GC time(ms)
    #       1               0.012               159240               212940                10647         0.00000000000001530000
    def self.result; end

    # The total time used for garbage collection in milliseconds
    def self.total_time; end
  end
end
