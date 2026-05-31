# frozen_string_literal: true

#  <code>WIN32OLE</code> objects represent OLE Automation object in Ruby.
#
#  By using WIN32OLE, you can access OLE server like VBScript.
#
#  Here is sample script.
#
#    require 'win32ole'
#
#    excel = WIN32OLE.new('Excel.Application')
#    excel.visible = true
#    workbook = excel.Workbooks.Add();
#    worksheet = workbook.Worksheets(1);
#    worksheet.Range("A1:D1").value = ["North","South","East","West"];
#    worksheet.Range("A2:B2").value = [5.2, 10];
#    worksheet.Range("C2").value = 8;
#    worksheet.Range("D2").value = 20;
#
#    range = worksheet.Range("A1:D2");
#    range.select
#    chart = workbook.Charts.Add;
#
#    workbook.saved = true;
#
#    excel.ActiveWorkbook.Close(0);
#    excel.Quit();
#
# Unfortunately, Win32OLE doesn't support the argument passed by
# reference directly.
# Instead, Win32OLE provides WIN32OLE::ARGV.
# If you want to get the result value of argument passed by reference,
# you can use WIN32OLE::ARGV.
#
#    oleobj.method(arg1, arg2, refargv3)
#    puts WIN32OLE::ARGV[2]   # the value of refargv3 after called oleobj.method
class WIN32OLE
  ARGV = _
  CP_ACP = _
  CP_MACCP = _
  CP_OEMCP = _
  CP_SYMBOL = _
  CP_THREAD_ACP = _
  CP_UTF7 = _
  CP_UTF8 = _
  LOCALE_SYSTEM_DEFAULT = _
  LOCALE_USER_DEFAULT = _
  VERSION = _

  # Returns current codepage.
  #    WIN32OLE.codepage # => WIN32OLE::CP_ACP
  def self.codepage; end

  # Sets current codepage.
  # The WIN32OLE.codepage is initialized according to
  # Encoding.default_internal.
  # If Encoding.default_internal is nil then WIN32OLE.codepage
  # is initialized according to Encoding.default_external.
  #
  #    WIN32OLE.codepage = WIN32OLE::CP_UTF8
  #    WIN32OLE.codepage = 65001
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

  # Creates GUID.
  #    WIN32OLE.create_guid # => {1CB530F1-F6B1-404D-BCE6-1959BF91F4A8}
  def self.create_guid; end

  # Returns current locale id (lcid). The default locale is
  # LOCALE_SYSTEM_DEFAULT.
  #
  #    lcid = WIN32OLE.locale
  def self.locale; end

  # Sets current locale id (lcid).
  #
  #    WIN32OLE.locale = 1033 # set locale English(U.S)
  #    obj = WIN32OLE_VARIANT.new("$100,000", WIN32OLE::VARIANT::VT_CY)
  def self.locale=(lcid) end

  # Invokes Release method of Dispatch interface of WIN32OLE object.
  # You should not use this method because this method
  # exists only for debugging WIN32OLE.
  # The return value is reference counter of OLE object.
  def self.ole_free(win32_ole) end

  # :nodoc
  def self.ole_initialize; end

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

  # :nodoc
  def self.ole_uninitialize; end

  # Returns a new WIN32OLE object(OLE Automation object).
  # The first argument server specifies OLE Automation server.
  # The first argument should be CLSID or PROGID.
  # If second argument host specified, then returns OLE Automation
  # object on host.
  #
  #     WIN32OLE.new('Excel.Application') # => Excel OLE Automation WIN32OLE object.
  #     WIN32OLE.new('{00024500-0000-0000-C000-000000000046}') # => Excel OLE Automation WIN32OLE object.
  def initialize(server, *host) end

  # Returns the value of Collection specified by a1, a2,....
  #
  #    dict = WIN32OLE.new('Scripting.Dictionary')
  #    dict.add('ruby', 'Ruby')
  #    puts dict['ruby'] # => 'Ruby' (same as `puts dict.item('ruby')')
  #
  # Remark: You can not use this method to get the property.
  #    excel = WIN32OLE.new('Excel.Application')
  #    # puts excel['Visible']  This is error !!!
  #    puts excel.Visible # You should to use this style to get the property.
  def [](a1, a2, *args) end

  # Sets the value to WIN32OLE object specified by a1, a2, ...
  #
  #    dict = WIN32OLE.new('Scripting.Dictionary')
  #    dict.add('ruby', 'RUBY')
  #    dict['ruby'] = 'Ruby'
  #    puts dict['ruby'] # => 'Ruby'
  #
  # Remark: You can not use this method to set the property value.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    # excel['Visible'] = true # This is error !!!
  #    excel.Visible = true # You should to use this style to set the property.
  def []=(a1, a2, *args, val) end

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
  # If and only if you received the exception "HRESULT error code:
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
  # The element of the array is property (settable) of WIN32OLE object.
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

  # Returns the array of WIN32OLE_METHOD object .
  # The element of the array is property (settable) of WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    properties = excel.ole_put_methods
  def ole_put_methods; end

  # Returns WIN32OLE object for a specific dispatch or dual
  # interface specified by iid.
  #
  #     ie = WIN32OLE.new('InternetExplorer.Application')
  #     ie_web_app = ie.ole_query_interface('{0002DF05-0000-0000-C000-000000000046}') # => WIN32OLE object for dispinterface IWebBrowserApp
  def ole_query_interface(iid) end

  # Returns true when OLE object has OLE method, otherwise returns false.
  #
  #     ie = WIN32OLE.new('InternetExplorer.Application')
  #     ie.ole_respond_to?("gohome") => true
  def ole_respond_to?(method) end

  # Returns WIN32OLE_TYPE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    tobj = excel.ole_type
  def ole_type; end
  alias ole_obj_help ole_type

  # Returns the WIN32OLE_TYPELIB object. The object represents the
  # type library which contains the WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    tlib = excel.ole_typelib
  #    puts tlib.name  # -> 'Microsoft Excel 9.0 Object Library'
  def ole_typelib; end

  # Sets property of OLE object.
  # When you want to set property with argument, you can use this method.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    excel.Visible = true
  #    book = excel.workbooks.add
  #    sheet = book.worksheets(1)
  #    sheet.setproperty('Cells', 1, 2, 10) # => The B1 cell value is 10.
  def setproperty(property, *args) end

  module VARIANT
    VT_ARRAY = _
    VT_BOOL = _
    VT_BSTR = _
    VT_BYREF = _
    VT_CY = _
    VT_DATE = _
    VT_DISPATCH = _
    VT_EMPTY = _
    VT_ERROR = _
    VT_I1 = _
    VT_I2 = _
    VT_I4 = _
    VT_I8 = _
    VT_INT = _
    VT_NULL = _
    VT_PTR = _
    VT_R4 = _
    VT_R8 = _
    VT_UI1 = _
    VT_UI2 = _
    VT_UI4 = _
    VT_UI8 = _
    VT_UINT = _
    VT_UNKNOWN = _
    VT_USERDEFINED = _
    VT_VARIANT = _
  end
end
