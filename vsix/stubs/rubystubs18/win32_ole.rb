# frozen_string_literal: true

# <code>WIN32OLE</code> objects represent OLE Automation object in Ruby.
class WIN32OLE
  ARGV = _
  CP_THREAD_ACP = _
  VERSION = _

  # Returns current codepage.
  #    WIN32OLE.codepage # => WIN32OLE::CP_ACP
  def self.codepage; end

  # Sets current codepage.
  #    WIN32OLE.codepage = WIN32OLE::CP_UTF8
  def self.codepage=(code_page) end

  # Returns running OLE Automation object or WIN32OLE object from moniker.
  # 1st argument should be OLE program id or class id or moniker.
  #
  #    WIN32OLE.connect('Excel.Application') # => WIN32OLE object which represents running Excel.
  def self.connect(ole) end

  # Defines the constants of OLE Automation server as mod's constants.
  # The first argument is WIN32OLE object or type library name.
  # If 2nd argument is omitted, the default is WIN32OLE.
  # The first letter of Ruby's constant variable name is upper case,
  # so constant variable name of WIN32OLE object is capitalized.
  # For example, the 'xlTop' constant of Excel is changed to 'XlTop'
  # in WIN32OLE.
  # If the first letter of constant variabl is not [A-Z], then
  # the constant is defined as CONSTANTS hash element.
  #
  #    module EXCEL_CONST
  #    end
  #    excel = WIN32OLE.new('Excel.Application')
  #    WIN32OLE.const_load(excel, EXCEL_CONST)
  #    puts EXCEL_CONST::XlTop # => -4160
  #    puts EXCEL_CONST::CONSTANTS['_xlDialogChartSourceData'] # => 541
  #
  #    WIN32OLE.const_load(excel)
  #    puts WIN32OLE::XlTop # => -4160
  #
  #    module MSO
  #    end
  #    WIN32OLE.const_load('Microsoft Office 9.0 Object Library', MSO)
  #    puts MSO::MsoLineSingle # => 1
  def self.const_load(ole, mod = WIN32OLE) end

  # Invokes Release method of Dispatch interface of WIN32OLE object.
  # You should not use this method because this method
  # exists only for debugging WIN32OLE.
  # The return value is reference counter of OLE object.
  def self.ole_free(win32_ole) end

  # Returns reference counter of Dispatch interface of WIN32OLE object.
  # You should not use this method because this method
  # exists only for debugging WIN32OLE.
  def self.ole_reference_count(win32_ole) end

  # Displays helpfile. The 1st argument specifies WIN32OLE_TYPE
  # object or WIN32OLE_METHOD object or helpfile.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    typeobj = excel.ole_type
  #    WIN32OLE.ole_show_help(typeobj)
  def self.ole_show_help(obj, *helpcontext) end

  # Returns a new WIN32OLE object(OLE Automation object).
  # The first argument server specifies OLE Automation server.
  # The first argument should be CLSID or PROGID.
  # If second argument host specified, then returns OLE Automation
  # object on host.
  #
  #     WIN32OLE.new('Excel.Application') # => Excel OLE Automation WIN32OLE object.
  #     WIN32OLE.new('{00024500-0000-0000-C000-000000000046}') # => Excel OLE Automation WIN32OLE object.
  def initialize(server, *host) end

  # Returns property of OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    puts excel['Visible'] # => false
  def [](property_name) end

  # Sets property of OLE object.
  # When you want to set property with argument, you can use this method.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    excel['Visible'] = true
  #    book = excel.workbooks.add
  #    sheet = book.worksheets(1)
  #    sheet.setproperty('Cells', 1, 2, 10) # => The B1 cell value is 10.
  def []=(*args) end
  alias setproperty []=

  # Runs the early binding method to get property.
  # The 1st argument specifies dispatch ID,
  # the 2nd argument specifies the array of arguments,
  # the 3rd argument specifies the array of the type of arguments.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    puts excel._getproperty(558, [], []) # same effect as puts excel.visible
  def _getproperty(dispid, args, types) end

  # Runs the early binding method.
  # The 1st argument specifies dispatch ID,
  # the 2nd argument specifies the array of arguments,
  # the 3rd argument specifies the array of the type of arguments.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    excel._invoke(302, [], []) #  same effect as excel.Quit
  def _invoke(dispid, args, types) end

  # Runs the early binding method to set property.
  # The 1st argument specifies dispatch ID,
  # the 2nd argument specifies the array of arguments,
  # the 3rd argument specifies the array of the type of arguments.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    excel._setproperty(558, [true], [WIN32OLE::VARIANT::VT_BOOL]) # same effect as excel.visible = true
  def _setproperty(dispid, args, types) end

  # Iterates over each item of OLE collection which has IEnumVARIANT interface.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    book = excel.workbooks.add
  #    sheets = book.worksheets(1)
  #    cells = sheets.cells("A1:A5")
  #    cells.each do |cell|
  #      cell.value = 10
  #    end
  def each; end

  # Runs OLE method.
  # The first argument specifies the method name of OLE Automation object.
  # The others specify argument of the <i>method</i>.
  # If you can not execute <i>method</i> directly, then use this method instead.
  #
  #   excel = WIN32OLE.new('Excel.Application')
  #   excel.invoke('Quit')  # => same as excel.Quit
  def invoke(method, *args) end

  # Calls WIN32OLE#invoke method.
  def method_missing(id, *args) end

  # Initialize WIN32OLE object(ActiveX Control) by calling
  # IPersistMemory::InitNew.
  #
  # Before calling OLE method, some kind of the ActiveX controls
  # created with MFC should be initialized by calling
  # IPersistXXX::InitNew.
  #
  # If and only if you recieved the exception "HRESULT error code:
  # 0x8000ffff catastrophic failure", try this method before
  # invoking any ole_method.
  #
  #    obj = WIN32OLE.new("ProgID_or_GUID_of_ActiveX_Control")
  #    obj.ole_activex_initialize
  #    obj.method(...)
  def ole_activex_initialize; end

  # invokes Release method of Dispatch interface of WIN32OLE object.
  # Usually, you do not need to call this method because Release method
  # called automatically when WIN32OLE object garbaged.
  def ole_free; end

  # Returns the array of WIN32OLE_METHOD object .
  # The element of the array is functional method of WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    properties = excel.ole_func_methods
  def ole_func_methods; end

  # Returns the array of WIN32OLE_METHOD object .
  # The element of the array is property (gettable) of WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    properties = excel.ole_get_methods
  def ole_get_methods; end

  # Returns WIN32OLE_METHOD object corresponding with method
  # specified by 1st argument.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    method = excel.ole_method_help('Quit')
  def ole_method(p1) end
  alias ole_method_help ole_method

  # Returns the array of WIN32OLE_METHOD object.
  # The element is OLE method of WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    methods = excel.ole_methods
  def ole_methods; end

  # Returns WIN32OLE_TYPE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    tobj = excel.ole_obj_help
  def ole_obj_help; end

  # Returns the array of WIN32OLE_METHOD object .
  # The element of the array is property (settable) of WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    properties = excel.ole_put_methods
  def ole_put_methods; end

  module VARIANT
    VT_ARRAY = _
    VT_BOOL = _
    VT_BSTR = _
    VT_BYREF = _
    VT_CY = _
    VT_DATE = _
    VT_DISPATCH = _
    VT_ERROR = _
    VT_I1 = _
    VT_I2 = _
    VT_I4 = _
    VT_INT = _
    VT_PTR = _
    VT_R4 = _
    VT_R8 = _
    VT_UI1 = _
    VT_UI2 = _
    VT_UI4 = _
    VT_UINT = _
    VT_UNKNOWN = _
    VT_USERDEFINED = _
    VT_VARIANT = _
  end
end
