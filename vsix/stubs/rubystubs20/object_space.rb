# frozen_string_literal: true

# The ObjectSpace module contains a number of routines
# that interact with the garbage collection facility and allow you to
# traverse all living objects with an iterator.
#
# ObjectSpace also provides support for object finalizers, procs that will be
# called when a specific object is about to be destroyed by garbage
# collection.
#
#    include ObjectSpace
#
#    a = "A"
#    b = "B"
#    c = "C"
#
#    define_finalizer(a, proc {|id| puts "Finalizer one on #{id}" })
#    define_finalizer(a, proc {|id| puts "Finalizer two on #{id}" })
#    define_finalizer(b, proc {|id| puts "Finalizer three on #{id}" })
#
# _produces:_
#
#    Finalizer three on 537763470
#    Finalizer one on 537763480
#    Finalizer two on 537763480
module ObjectSpace
  # Converts an object id to a reference to the object. May not be
  # called on an object id passed as a parameter to a finalizer.
  #
  #    s = "I am a string"                    #=> "I am a string"
  #    r = ObjectSpace._id2ref(s.object_id)   #=> "I am a string"
  #    r == s                                 #=> true
  def self._id2ref(object_id) end

  # Counts objects for each type.
  #
  # It returns a hash, such as:
  #     {
  #       :TOTAL=>10000,
  #       :FREE=>3011,
  #       :T_OBJECT=>6,
  #       :T_CLASS=>404,
  #       # ...
  #     }
  #
  # The contents of the returned hash are implementation specific.
  # It may be changed in future.
  #
  # If the optional argument +result_hash+ is given,
  # it is overwritten and returned. This is intended to avoid probe effect.
  #
  # This method is only expected to work on C Ruby.
  def self.count_objects(*result_hash) end

  # Adds <i>aProc</i> as a finalizer, to be called after <i>obj</i>
  # was destroyed.
  def self.define_finalizer(obj, proc = _) end

  # Calls the block once for each living, nonimmediate object in this
  # Ruby process. If <i>module</i> is specified, calls the block
  # for only those classes or modules that match (or are a subclass of)
  # <i>module</i>. Returns the number of objects found. Immediate
  # objects (<code>Fixnum</code>s, <code>Symbol</code>s
  # <code>true</code>, <code>false</code>, and <code>nil</code>) are
  # never returned. In the example below, <code>each_object</code>
  # returns both the numbers we defined and several constants defined in
  # the <code>Math</code> module.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    a = 102.7
  #    b = 95       # Won't be returned
  #    c = 12345678987654321
  #    count = ObjectSpace.each_object(Numeric) {|x| p x }
  #    puts "Total count: #{count}"
  #
  # <em>produces:</em>
  #
  #    12345678987654321
  #    102.7
  #    2.71828182845905
  #    3.14159265358979
  #    2.22044604925031e-16
  #    1.7976931348623157e+308
  #    2.2250738585072e-308
  #    Total count: 7
  def self.each_object(*module1) end

  # Initiates garbage collection, unless manually disabled.
  def self.garbage_collect; end

  # Removes all finalizers for <i>obj</i>.
  def self.undefine_finalizer(obj) end

  private

  # Converts an object id to a reference to the object. May not be
  # called on an object id passed as a parameter to a finalizer.
  #
  #    s = "I am a string"                    #=> "I am a string"
  #    r = ObjectSpace._id2ref(s.object_id)   #=> "I am a string"
  #    r == s                                 #=> true
  def _id2ref(object_id) end

  # Counts objects for each type.
  #
  # It returns a hash, such as:
  #     {
  #       :TOTAL=>10000,
  #       :FREE=>3011,
  #       :T_OBJECT=>6,
  #       :T_CLASS=>404,
  #       # ...
  #     }
  #
  # The contents of the returned hash are implementation specific.
  # It may be changed in future.
  #
  # If the optional argument +result_hash+ is given,
  # it is overwritten and returned. This is intended to avoid probe effect.
  #
  # This method is only expected to work on C Ruby.
  def count_objects(*result_hash) end

  # Adds <i>aProc</i> as a finalizer, to be called after <i>obj</i>
  # was destroyed.
  def define_finalizer(p1, p2 = v2) end

  # Calls the block once for each living, nonimmediate object in this
  # Ruby process. If <i>module</i> is specified, calls the block
  # for only those classes or modules that match (or are a subclass of)
  # <i>module</i>. Returns the number of objects found. Immediate
  # objects (<code>Fixnum</code>s, <code>Symbol</code>s
  # <code>true</code>, <code>false</code>, and <code>nil</code>) are
  # never returned. In the example below, <code>each_object</code>
  # returns both the numbers we defined and several constants defined in
  # the <code>Math</code> module.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    a = 102.7
  #    b = 95       # Won't be returned
  #    c = 12345678987654321
  #    count = ObjectSpace.each_object(Numeric) {|x| p x }
  #    puts "Total count: #{count}"
  #
  # <em>produces:</em>
  #
  #    12345678987654321
  #    102.7
  #    2.71828182845905
  #    3.14159265358979
  #    2.22044604925031e-16
  #    1.7976931348623157e+308
  #    2.2250738585072e-308
  #    Total count: 7
  def each_object(p1 = v1) end

  # Initiates garbage collection, unless manually disabled.
  def garbage_collect; end

  # Removes all finalizers for <i>obj</i>.
  def undefine_finalizer(obj) end

  # An ObjectSpace::WeakMap object holds references to
  # any objects, but those objects can get garbage collected.
  #
  # This class is mostly used internally by WeakRef, please use
  # +lib/weakref.rb+ for the public interface.
  class WeakMap
    # Retrieves a weakly referenced object with the given key
    def [](p1) end

    # Creates a weak reference from the given key to the given value
    def []=(p1, p2) end

    private

    def finalize(p1) end
  end
end
