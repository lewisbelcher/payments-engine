use std::fs::{self, File};
use std::io::Write;

use anyhow::Result;
use payments_engine::process;

static EXAMPLES: [(&str, Option<&str>); 7] = [
	("examples/complex.csv", Some("examples/complex.output.csv")),
	(
		"examples/locked-account.csv",
		Some("examples/locked-account.output.csv"),
	),
	("examples/simple.csv", Some("examples/simple.output.csv")),
	("examples/unseen.csv", Some("examples/unseen.output.csv")),
	("examples/invalid-header.csv", None),
	("examples/invalid-float.csv", None),
	("examples/invalid-type.csv", None),
];

#[test]
fn test_all_examples() -> Result<()> {
	for (input_path, expected_path) in EXAMPLES.iter() {
		test_example_file(input_path, expected_path)?;
	}
	Ok(())
}

fn test_example_file(input_path: &str, expected_path: &Option<&str>) -> Result<()> {
	let mut input = File::open(input_path)?;
	let mut output = Vec::new();
	let result = process::run(&mut input, &mut output);

	if let Some(expected_path) = expected_path {
		let expected = fs::read_to_string(expected_path)?;
		let result = std::str::from_utf8(&output)?;
		assert_eq!(sorted(&expected), sorted(result));
	} else {
		assert!(result.is_err());
	}
	Ok(())
}

fn sorted(s: &str) -> Vec<&str> {
	let mut list: Vec<&str> = s.split_ascii_whitespace().collect();
	list.sort_unstable();
	list
}

#[test]
#[ignore] // Enable to test against large amounts of input
fn test_large_csv() {
	env_logger::init();
	struct BigCsv {
		supplied_lines: u32,
		max_lines: u32,
	}

	impl std::io::Read for BigCsv {
		fn read(&mut self, mut buf: &mut [u8]) -> Result<usize, std::io::Error> {
			if self.max_lines <= self.supplied_lines {
				return Ok(0);
			}
			let n = if self.supplied_lines == 0 {
				buf.write(b"type,client,tx,amount\n")?
			} else {
				buf.write(&format!("deposit,1,{},1.0\n", self.supplied_lines).into_bytes())?
			};
			self.supplied_lines += 1;
			Ok(n)
		}
	}
	let n = 1000000000;
	let mut input = BigCsv {
		supplied_lines: 0,
		max_lines: n,
	};
	let mut output = Vec::new();
	let result = process::run(&mut input, &mut output);
	assert!(result.is_ok(), "{:?}", result);
	if let Ok(output) = std::str::from_utf8(&output) {
		assert_eq!(
			output,
			format!(
				"client,available,held,total,locked\n1,{}.0000,0.0000,{}.0000,false\n",
				n - 1,
				n - 1
			)
		);
	} else {
		assert!(false, "Failed to process input");
	}
}
