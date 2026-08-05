//! Hover and receiver resolution for nonlocal variables.

use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn instance_variable_hover_and_receiver_resolution_are_source_ordered() {
    check(
        r#"
class Types
  def convert
    @value = "early"
    @value<hover label="String">.upcase<hover label="String">
    @value = 1
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn instance_variable_receiver_does_not_borrow_another_owner_type() {
    check(
        r#"
class First
  def write
    @value = "first"
  end
end

class Second
  def read
    @value.upcase<hover label="?">
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn unknown_instance_variable_reassignment_invalidates_earlier_concrete_type() {
    check(
        r#"
class Types
  def convert
    @value = "early"
    @value = dynamic_value
    @value<hover label="Unknown[unresolved_assignment_value]">.upcase<hover label="?">
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn nonlocal_read_inside_assignment_rhs_observes_the_previous_write() {
    check(
        r#"
class Types
  def convert
    @value = "early"
    @value = @value<hover label="String">.missing
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn removing_a_write_does_not_leave_a_stale_read_type_after_edit() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "variable_read_lifecycle.rb",
            r#"class Types
  def read
    @value = "ready"
    @value<hover label="String">
  end
end
"#,
        )
        .await;

    editor
        .set_and_check_fixture(
            "variable_read_lifecycle.rb",
            r#"class Types
  def read
    @value<hover label="Unknown[no_reaching_assignment]">
  end
end
"#,
        )
        .await;
}
