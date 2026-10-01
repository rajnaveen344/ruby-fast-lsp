//! Propagation of value-constant types into local flow inference.

use crate::test::harness::{check, check_multi_file};

#[tokio::test]
async fn namespaced_string_constant_keeps_its_value_type_in_method_flow() {
    check(
        r#"
module PaymentErrors
  CAPTURE_FAILED<hint label="String"> = "1647".freeze
  MULTIPLE_CAPTURE_FAILED<hint label="String"> = "1646".freeze
end

class Payment
  def capture(condition)<hint label=" -> String">
    if condition
      error_code<hint label=": String"> = PaymentErrors::CAPTURE_FAILED
    else
      error_code<hint label=": String"> = PaymentErrors::MULTIPLE_CAPTURE_FAILED
    end
    error_code<hover label="String">
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn late_indexed_value_constant_refreshes_existing_method_flow() {
    check_multi_file(&[
        (
            "payment.rb",
            r#"
class Payment
  def capture(condition)<hint label=" -> String">
    if condition
      error_code<hint label=": String"> = PaymentErrors::CAPTURE_FAILED
    else
      error_code<hint label=": String"> = PaymentErrors::MULTIPLE_CAPTURE_FAILED
    end
    error_code<hover label="String">
  end
end
"#,
        ),
        (
            "payment_errors.rb",
            r#"
module PaymentErrors
  CAPTURE_FAILED<hint label="String"> = "1647".freeze
  MULTIPLE_CAPTURE_FAILED<hint label="String"> = "1646".freeze
end
"#,
        ),
    ])
    .await;
}
