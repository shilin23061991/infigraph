use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TestTemplate {
    pub test_type: String,
    pub conventions: String,
    pub scaffold: String,
}

pub fn test_templates_for(framework: &str, test_type: Option<&str>) -> Vec<TestTemplate> {
    let fw = framework.to_lowercase();
    let types: &[&str] = match test_type {
        Some(t) => &[normalize_type(t)],
        None => &["unit", "integration", "functional", "e2e"],
    };

    let mut out = Vec::new();
    for &tt in types {
        if let Some(tpl) = match_template(&fw, tt) {
            out.push(tpl);
        }
    }
    out
}

fn normalize_type(s: &str) -> &'static str {
    match s {
        "unit" => "unit",
        "integration" => "integration",
        "functional" => "functional",
        "e2e" => "e2e",
        _ => "unit",
    }
}

fn match_template(fw: &str, test_type: &str) -> Option<TestTemplate> {
    let (conv, scaffold) = if fw.contains("rust") || fw.contains("cargo") {
        rust_template(test_type)?
    } else if fw.contains("pytest") || (fw.contains("python") && !fw.contains("unittest")) {
        pytest_template(test_type)?
    } else if fw.contains("unittest") {
        python_unittest_template(test_type)?
    } else if fw.contains("junit") || fw.contains("java") {
        junit_template(test_type)?
    } else if fw.contains("jest") || fw.contains("vitest") {
        jest_template(test_type)?
    } else if fw.contains("mocha") {
        mocha_template(test_type)?
    } else if fw.contains("karate") {
        karate_template(test_type)?
    } else if fw.contains("playwright") || fw.contains("cypress") {
        playwright_template(test_type)?
    } else if fw.contains("go") {
        go_template(test_type)?
    } else if fw.contains("nunit")
        || fw.contains("xunit")
        || fw.contains("mstest")
        || fw.contains("csharp")
    {
        csharp_template(fw, test_type)?
    } else if fw.contains("kotlin") || fw.contains("kotest") {
        kotlin_template(test_type)?
    } else if fw.contains("scala") {
        scala_template(test_type)?
    } else if fw.contains("rspec") {
        rspec_template(test_type)?
    } else if fw.contains("minitest") || (fw.contains("ruby") && !fw.contains("rspec")) {
        minitest_template(test_type)?
    } else if fw.contains("swift") || fw.contains("xctest") {
        swift_template(test_type)?
    } else if fw.contains("elixir") || fw.contains("exunit") {
        elixir_template(test_type)?
    } else if fw.contains("testng") {
        testng_template(test_type)?
    } else if fw.contains("cucumber") || fw.contains("gherkin") {
        cucumber_template(test_type)?
    } else {
        return None;
    };
    Some(TestTemplate {
        test_type: test_type.to_string(),
        conventions: conv.to_string(),
        scaffold: scaffold.to_string(),
    })
}

fn rust_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use #[test] or #[rstest] for parametrized. Mocks via mockall. Assert with assert_eq!/assert!. Place in mod tests at bottom of file.",
            r#"#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_FUNCTION_NAME_SCENARIO() {
        // arrange
        let input = TODO;
        // act
        let result = FUNCTION_NAME(input);
        // assert
        assert_eq!(result, EXPECTED);
    }
}"#,
        )),
        "integration" => Some((
            "Place in tests/ directory. Each file is a separate crate. Use #[tokio::test] for async. No mod tests wrapper needed.",
            r#"use CRATE_NAME::FUNCTION_NAME;

#[test]
fn test_SCENARIO_integration() {
    // setup: real dependencies
    let state = setup();
    // act
    let result = FUNCTION_NAME(&state);
    // assert
    assert!(result.is_ok());
    // teardown if needed
}"#,
        )),
        "e2e" => Some((
            "Use assert_cmd for CLI e2e. Or spawn server + reqwest for HTTP services. Place in tests/.",
            r#"use assert_cmd::Command;

#[test]
fn test_SCENARIO_e2e() {
    let mut cmd = Command::cargo_bin("BINARY_NAME").unwrap();
    cmd.arg("SUBCOMMAND")
        .arg("--flag=VALUE")
        .assert()
        .success()
        .stdout(predicates::str::contains("EXPECTED"));
}"#,
        )),
        _ => None,
    }
}

fn pytest_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use @pytest.mark.parametrize for data-driven. Fixtures for setup/teardown. Assert with plain assert. Name files test_*.py.",
            r#"import pytest

def test_FUNCTION_NAME_SCENARIO():
    # arrange
    input_val = TODO
    # act
    result = FUNCTION_NAME(input_val)
    # assert
    assert result == EXPECTED

@pytest.mark.parametrize("input_val,expected", [
    (CASE1_IN, CASE1_OUT),
    (CASE2_IN, CASE2_OUT),
])
def test_FUNCTION_NAME_parametrized(input_val, expected):
    assert FUNCTION_NAME(input_val) == expected"#,
        )),
        "integration" => Some((
            "Use fixtures with scope='session' or 'module' for expensive setup (DB, API clients). conftest.py for shared fixtures. Mark with @pytest.mark.integration.",
            r#"import pytest

@pytest.fixture(scope="module")
def db_connection():
    conn = create_connection()
    yield conn
    conn.close()

@pytest.mark.integration
def test_SCENARIO_integration(db_connection):
    result = FUNCTION_NAME(db_connection)
    assert result is not None"#,
        )),
        "functional" => Some((
            "Test full feature workflows. Use fixtures for app/client setup. Mark with @pytest.mark.functional.",
            r#"import pytest

@pytest.fixture
def client(app):
    return app.test_client()

@pytest.mark.functional
def test_FEATURE_workflow(client):
    # step 1
    resp = client.post("/endpoint", json=PAYLOAD)
    assert resp.status_code == 200
    # step 2: verify side effects
    result = client.get("/endpoint/" + resp.json["id"])
    assert result.json["status"] == "created""#,
        )),
        "e2e" => Some((
            "Full stack tests. Use subprocess or docker fixtures. Mark with @pytest.mark.e2e. Often slow — separate from unit runs.",
            r#"import pytest
import subprocess

@pytest.mark.e2e
def test_SCENARIO_e2e():
    result = subprocess.run(["python", "-m", "MODULE", "ARGS"], capture_output=True, text=True)
    assert result.returncode == 0
    assert "EXPECTED_OUTPUT" in result.stdout"#,
        )),
        _ => None,
    }
}

fn python_unittest_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Subclass unittest.TestCase. setUp/tearDown for fixtures. self.assertEqual/assertTrue for assertions.",
            r#"import unittest

class TestFUNCTION_NAME(unittest.TestCase):
    def setUp(self):
        self.input = TODO

    def test_SCENARIO(self):
        result = FUNCTION_NAME(self.input)
        self.assertEqual(result, EXPECTED)"#,
        )),
        "integration" => Some((
            "Use setUpClass/tearDownClass for expensive resources. Subclass TestCase.",
            r#"import unittest

class TestIntegrationSCENARIO(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.conn = create_connection()

    @classmethod
    def tearDownClass(cls):
        cls.conn.close()

    def test_SCENARIO(self):
        result = FUNCTION_NAME(self.conn)
        self.assertIsNotNone(result)"#,
        )),
        _ => None,
    }
}

fn junit_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use @Test. @ParameterizedTest + @CsvSource for data-driven. Mockito for mocks. AssertJ or JUnit assertions. Place in src/test/java/.",
            r#"import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class FunctionNameTest {
    @Test
    void shouldRETURN_EXPECTED_whenSCENARIO() {
        // arrange
        var input = TODO;
        // act
        var result = functionName(input);
        // assert
        assertEquals(EXPECTED, result);
    }
}"#,
        )),
        "integration" => Some((
            "Use @SpringBootTest or @ExtendWith for DI. @Testcontainers for DB/services. @Transactional for rollback.",
            r#"import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.beans.factory.annotation.Autowired;

@SpringBootTest
class FunctionNameIntegrationTest {
    @Autowired
    private ServiceName service;

    @Test
    void shouldSCENARIO() {
        var result = service.functionName(INPUT);
        assertNotNull(result);
    }
}"#,
        )),
        "functional" => Some((
            "Use @SpringBootTest(webEnvironment=RANDOM_PORT) + TestRestTemplate. Test full HTTP flows.",
            r#"import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.web.client.TestRestTemplate;
import org.springframework.beans.factory.annotation.Autowired;

@SpringBootTest(webEnvironment = SpringBootTest.WebEnvironment.RANDOM_PORT)
class FunctionNameFunctionalTest {
    @Autowired
    private TestRestTemplate restTemplate;

    @Test
    void shouldCOMPLETE_WORKFLOW() {
        var response = restTemplate.postForEntity("/endpoint", PAYLOAD, ResponseType.class);
        assertEquals(200, response.getStatusCodeValue());
    }
}"#,
        )),
        "e2e" => Some((
            "Use Selenium WebDriver or TestContainers. @Tag(\"e2e\") to separate. Full stack with real browser/services.",
            r#"import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.Tag;
import org.openqa.selenium.WebDriver;
import org.openqa.selenium.chrome.ChromeDriver;

@Tag("e2e")
class FunctionNameE2ETest {
    @Test
    void shouldCOMPLETE_FLOW() {
        WebDriver driver = new ChromeDriver();
        try {
            driver.get("http://localhost:8080/page");
            // interact and assert
        } finally {
            driver.quit();
        }
    }
}"#,
        )),
        _ => None,
    }
}

fn jest_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use describe/it/expect. jest.mock() for module mocks. jest.fn() for function spies. Place in __tests__/ or co-located *.test.ts.",
            r#"describe('functionName', () => {
  it('should EXPECTED when SCENARIO', () => {
    // arrange
    const input = TODO;
    // act
    const result = functionName(input);
    // assert
    expect(result).toBe(EXPECTED);
  });
});"#,
        )),
        "integration" => Some((
            "Use supertest for HTTP. beforeAll/afterAll for setup/teardown. jest.setTimeout for slow tests.",
            r#"import request from 'supertest';
import { app } from '../src/app';

describe('ENDPOINT integration', () => {
  beforeAll(async () => { await setupDb(); });
  afterAll(async () => { await teardownDb(); });

  it('should SCENARIO', async () => {
    const res = await request(app).post('/endpoint').send(PAYLOAD);
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty('id');
  });
});"#,
        )),
        "e2e" => Some((
            "Use Playwright or Puppeteer via jest-playwright. Or jest with full server spawn.",
            r#"describe('E2E: FEATURE', () => {
  beforeAll(async () => { await startServer(); });
  afterAll(async () => { await stopServer(); });

  it('should COMPLETE_FLOW', async () => {
    const res = await fetch(`${BASE_URL}/endpoint`, { method: 'POST', body: JSON.stringify(PAYLOAD) });
    expect(res.ok).toBe(true);
  });
});"#,
        )),
        _ => None,
    }
}

fn mocha_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use describe/it with chai expect/assert. sinon for stubs/spies. Place in test/ directory.",
            r#"const { expect } = require('chai');

describe('functionName', () => {
  it('should EXPECTED when SCENARIO', () => {
    const result = functionName(INPUT);
    expect(result).to.equal(EXPECTED);
  });
});"#,
        )),
        "integration" => Some((
            "Use before/after hooks. chai-http for HTTP tests. sinon for external service stubs.",
            r#"const { expect } = require('chai');
const chaiHttp = require('chai-http');
const app = require('../src/app');

describe('ENDPOINT integration', () => {
  before(async () => { await setupDb(); });
  after(async () => { await teardownDb(); });

  it('should SCENARIO', async () => {
    const res = await chai.request(app).post('/endpoint').send(PAYLOAD);
    expect(res).to.have.status(200);
  });
});"#,
        )),
        _ => None,
    }
}

fn karate_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "functional" | "integration" => Some((
            "Use Feature/Scenario/Given-When-Then. karate.call() for reusable steps. Background for shared setup. Match for JSON assertions. Place in src/test/java/ with .feature extension.",
            r#"Feature: FEATURE_NAME

  Background:
    * url baseUrl
    * header Content-Type = 'application/json'

  Scenario: SCENARIO_DESCRIPTION
    Given path '/endpoint'
    And request { "field": "value" }
    When method post
    Then status 200
    And match response.id == '#notnull'
    And match response.status == 'created'"#,
        )),
        "e2e" => Some((
            "Chain multiple API calls. Use def for variables. call for sub-features. configure retry for polling.",
            r#"Feature: E2E WORKFLOW_NAME

  Scenario: FULL_FLOW
    # Step 1: Create resource
    Given url baseUrl + '/resource'
    And request { "name": "test" }
    When method post
    Then status 201
    * def resourceId = response.id

    # Step 2: Verify resource
    Given url baseUrl + '/resource/' + resourceId
    When method get
    Then status 200
    And match response.name == 'test'"#,
        )),
        _ => None,
    }
}

fn playwright_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "e2e" | "functional" => Some((
            "Use test/expect from @playwright/test. Page object model for reusable selectors. test.beforeEach for navigation. Locators over selectors.",
            r#"import { test, expect } from '@playwright/test';

test.describe('FEATURE_NAME', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should SCENARIO', async ({ page }) => {
    await page.getByRole('button', { name: 'ACTION' }).click();
    await expect(page.getByText('EXPECTED')).toBeVisible();
  });
});"#,
        )),
        "integration" => Some((
            "Use API testing via request context. No browser needed. test.use({ baseURL }) for config.",
            r#"import { test, expect } from '@playwright/test';

test.describe('API: FEATURE', () => {
  test('should SCENARIO', async ({ request }) => {
    const response = await request.post('/api/endpoint', { data: PAYLOAD });
    expect(response.ok()).toBeTruthy();
    const body = await response.json();
    expect(body.id).toBeDefined();
  });
});"#,
        )),
        _ => None,
    }
}

fn go_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use testing.T. Table-driven tests with subtests. t.Helper() for test helpers. Place in same package as *_test.go.",
            r#"func TestFunctionName(t *testing.T) {
	tests := []struct {
		name     string
		input    InputType
		expected OutputType
	}{
		{"scenario1", INPUT1, EXPECTED1},
		{"scenario2", INPUT2, EXPECTED2},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := FunctionName(tt.input)
			if got != tt.expected {
				t.Errorf("FunctionName(%v) = %v, want %v", tt.input, got, tt.expected)
			}
		})
	}
}"#,
        )),
        "integration" => Some((
            "Use TestMain for setup/teardown. Build tags (//go:build integration) to separate. t.Skip if deps unavailable.",
            r#"//go:build integration

func TestMain(m *testing.M) {
	// setup
	db := setupTestDB()
	defer db.Close()
	os.Exit(m.Run())
}

func TestFunctionName_Integration(t *testing.T) {
	result, err := FunctionName(testDB)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result == nil {
		t.Fatal("expected non-nil result")
	}
}"#,
        )),
        "e2e" => Some((
            "Use TestMain + httptest.NewServer or exec.Command for full binary. Build tags //go:build e2e.",
            r#"//go:build e2e

func TestFunctionName_E2E(t *testing.T) {
	cmd := exec.Command("./binary", "arg1", "arg2")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("command failed: %v\n%s", err, out)
	}
	if !strings.Contains(string(out), "EXPECTED") {
		t.Errorf("expected output to contain EXPECTED, got: %s", out)
	}
}"#,
        )),
        _ => None,
    }
}

fn csharp_template(fw: &str, tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => {
            if fw.contains("xunit") {
                Some((
                    "Use [Fact] for single cases, [Theory]+[InlineData] for parametrized. Moq for mocks. FluentAssertions optional.",
                    r#"public class FunctionNameTests
{
    [Fact]
    public void FunctionName_ShouldRETURN_WhenSCENARIO()
    {
        // Arrange
        var input = TODO;
        // Act
        var result = FunctionName(input);
        // Assert
        Assert.Equal(EXPECTED, result);
    }

    [Theory]
    [InlineData(INPUT1, EXPECTED1)]
    [InlineData(INPUT2, EXPECTED2)]
    public void FunctionName_Parametrized(InputType input, OutputType expected)
    {
        Assert.Equal(expected, FunctionName(input));
    }
}"#,
                ))
            } else if fw.contains("nunit") {
                Some((
                    "Use [Test] for single, [TestCase] for parametrized. Assert.That with constraints. [SetUp]/[TearDown] for lifecycle.",
                    r#"[TestFixture]
public class FunctionNameTests
{
    [Test]
    public void FunctionName_ShouldRETURN_WhenSCENARIO()
    {
        var result = FunctionName(INPUT);
        Assert.That(result, Is.EqualTo(EXPECTED));
    }

    [TestCase(INPUT1, EXPECTED1)]
    [TestCase(INPUT2, EXPECTED2)]
    public void FunctionName_Parametrized(InputType input, OutputType expected)
    {
        Assert.That(FunctionName(input), Is.EqualTo(expected));
    }
}"#,
                ))
            } else {
                Some((
                    "Use [TestMethod]. [DataRow] for parametrized. MSTest assertions. [TestInitialize]/[TestCleanup] for lifecycle.",
                    r#"[TestClass]
public class FunctionNameTests
{
    [TestMethod]
    public void FunctionName_ShouldRETURN_WhenSCENARIO()
    {
        var result = FunctionName(INPUT);
        Assert.AreEqual(EXPECTED, result);
    }
}"#,
                ))
            }
        }
        "integration" => Some((
            "Use WebApplicationFactory<T> for ASP.NET. IClassFixture/ICollectionFixture for shared state. Testcontainers.NET for DB.",
            r#"public class FunctionNameIntegrationTests : IClassFixture<WebApplicationFactory<Program>>
{
    private readonly HttpClient _client;

    public FunctionNameIntegrationTests(WebApplicationFactory<Program> factory)
    {
        _client = factory.CreateClient();
    }

    [Fact]
    public async Task ShouldSCENARIO()
    {
        var response = await _client.PostAsJsonAsync("/endpoint", PAYLOAD);
        response.EnsureSuccessStatusCode();
    }
}"#,
        )),
        "e2e" => Some((
            "Use Playwright for .NET or Selenium WebDriver. WebApplicationFactory + browser automation.",
            r#"using Microsoft.Playwright;

[TestClass]
public class FunctionNameE2ETests
{
    [TestMethod]
    public async Task ShouldCOMPLETE_FLOW()
    {
        using var playwright = await Playwright.CreateAsync();
        await using var browser = await playwright.Chromium.LaunchAsync();
        var page = await browser.NewPageAsync();
        await page.GotoAsync("http://localhost:5000/page");
        await page.ClickAsync("text=ACTION");
        await Expect(page.Locator("text=EXPECTED")).ToBeVisibleAsync();
    }
}"#,
        )),
        _ => None,
    }
}

fn kotlin_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use @Test from kotlin.test or JUnit5. kotest for property-based and BDD style. assertEquals/assertNotNull from kotlin.test.",
            r#"import kotlin.test.Test
import kotlin.test.assertEquals

class FunctionNameTest {
    @Test
    fun `should EXPECTED when SCENARIO`() {
        val input = TODO
        val result = functionName(input)
        assertEquals(EXPECTED, result)
    }
}"#,
        )),
        "integration" => Some((
            "Use @SpringBootTest with Kotlin. Testcontainers for services. @Transactional for DB rollback.",
            r#"import org.junit.jupiter.api.Test
import org.springframework.boot.test.context.SpringBootTest
import org.springframework.beans.factory.annotation.Autowired
import kotlin.test.assertNotNull

@SpringBootTest
class FunctionNameIntegrationTest {
    @Autowired
    lateinit var service: ServiceName

    @Test
    fun `should SCENARIO`() {
        val result = service.functionName(INPUT)
        assertNotNull(result)
    }
}"#,
        )),
        _ => None,
    }
}

fn scala_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use AnyFlatSpec or AnyFunSuite from ScalaTest. Matchers for fluent assertions. ScalaMock for mocks.",
            r#"import org.scalatest.flatspec.AnyFlatSpec
import org.scalatest.matchers.should.Matchers

class FunctionNameSpec extends AnyFlatSpec with Matchers {
  "functionName" should "EXPECTED when SCENARIO" in {
    val input = TODO
    val result = functionName(input)
    result shouldBe EXPECTED
  }
}"#,
        )),
        "integration" => Some((
            "Use BeforeAndAfterAll for setup/teardown. Testcontainers-scala for services. Tag with IntegrationTest.",
            r#"import org.scalatest.flatspec.AnyFlatSpec
import org.scalatest.matchers.should.Matchers
import org.scalatest.BeforeAndAfterAll

class FunctionNameIntegrationSpec extends AnyFlatSpec with Matchers with BeforeAndAfterAll {
  override def beforeAll(): Unit = { /* setup */ }
  override def afterAll(): Unit = { /* teardown */ }

  "functionName" should "SCENARIO" taggedAs IntegrationTest in {
    val result = functionName(realDependency)
    result should not be null
  }
}"#,
        )),
        _ => None,
    }
}

fn rspec_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use describe/it/expect. let for lazy setup. before/after for hooks. Place in spec/ directory. Name *_spec.rb.",
            r#"RSpec.describe FunctionName do
  describe '#function_name' do
    let(:input) { TODO }

    it 'returns EXPECTED when SCENARIO' do
      result = function_name(input)
      expect(result).to eq(EXPECTED)
    end
  end
end"#,
        )),
        "integration" => Some((
            "Use database_cleaner for DB tests. before(:suite) for expensive setup. Tag with :integration.",
            r#"RSpec.describe FunctionName, :integration do
  before(:suite) { setup_database }
  after(:suite) { teardown_database }

  it 'persists SCENARIO' do
    result = function_name(real_connection)
    expect(result).not_to be_nil
  end
end"#,
        )),
        "e2e" => Some((
            "Use Capybara with Selenium/Playwright driver. feature/scenario DSL. Place in spec/features/.",
            r#"require 'capybara/rspec'

RSpec.feature 'FEATURE_NAME', :e2e do
  scenario 'SCENARIO' do
    visit '/page'
    click_button 'ACTION'
    expect(page).to have_content('EXPECTED')
  end
end"#,
        )),
        _ => None,
    }
}

fn minitest_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Subclass Minitest::Test. setup/teardown for lifecycle. assert_equal/assert_nil for assertions. Name test_*.rb.",
            r#"require 'minitest/autorun'

class TestFunctionName < Minitest::Test
  def setup
    @input = TODO
  end

  def test_SCENARIO
    result = function_name(@input)
    assert_equal EXPECTED, result
  end
end"#,
        )),
        "integration" => Some((
            "Use setup/teardown for DB connections. Minitest::Test subclass with real dependencies.",
            r#"require 'minitest/autorun'

class TestFunctionNameIntegration < Minitest::Test
  def setup
    @conn = create_connection
  end

  def teardown
    @conn.close
  end

  def test_SCENARIO
    result = function_name(@conn)
    refute_nil result
  end
end"#,
        )),
        _ => None,
    }
}

fn swift_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Subclass XCTestCase. XCTAssertEqual/XCTAssertTrue for assertions. setUp/tearDown for lifecycle. @testable import for internal access.",
            r#"import XCTest
@testable import MODULE_NAME

class FunctionNameTests: XCTestCase {
    func testSCENARIO() {
        let input = TODO
        let result = functionName(input)
        XCTAssertEqual(result, EXPECTED)
    }
}"#,
        )),
        "integration" => Some((
            "Use setUp/tearDown for real resources. expectation(description:) + waitForExpectations for async. @testable import.",
            r#"import XCTest
@testable import MODULE_NAME

class FunctionNameIntegrationTests: XCTestCase {
    var service: ServiceName!

    override func setUp() {
        super.setUp()
        service = ServiceName(config: .test)
    }

    override func tearDown() {
        service = nil
        super.tearDown()
    }

    func testSCENARIO() async throws {
        let result = try await service.functionName(INPUT)
        XCTAssertNotNil(result)
    }
}"#,
        )),
        _ => None,
    }
}

fn elixir_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use ExUnit.Case. assert/refute for assertions. setup/setup_all for lifecycle. Doctests for simple cases. Name *_test.exs in test/.",
            r#"defmodule FunctionNameTest do
  use ExUnit.Case

  describe "function_name/1" do
    test "returns EXPECTED when SCENARIO" do
      result = ModuleName.function_name(INPUT)
      assert result == EXPECTED
    end
  end
end"#,
        )),
        "integration" => Some((
            "Use Ecto.Adapters.SQL.Sandbox for DB. setup block with checkout. @tag :integration.",
            r#"defmodule FunctionNameIntegrationTest do
  use ExUnit.Case
  alias Ecto.Adapters.SQL.Sandbox

  setup do
    :ok = Sandbox.checkout(Repo)
    Sandbox.mode(Repo, {:shared, self()})
    :ok
  end

  @tag :integration
  test "SCENARIO" do
    result = ModuleName.function_name(real_input)
    assert result != nil
  end
end"#,
        )),
        "functional" => Some((
            "Use ConnTest for Phoenix endpoints. build_conn() + dispatch. JSON assertions with json_response/2.",
            r#"defmodule AppWeb.FunctionNameControllerTest do
  use AppWeb.ConnCase

  test "POST /endpoint creates resource", %{conn: conn} do
    conn = post(conn, "/endpoint", %{field: "value"})
    assert %{"id" => id} = json_response(conn, 201)

    conn = get(conn, "/endpoint/#{id}")
    assert %{"status" => "created"} = json_response(conn, 200)
  end
end"#,
        )),
        _ => None,
    }
}

fn testng_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "unit" => Some((
            "Use @Test. @DataProvider for parametrized. Assert from TestNG. groups for categorization.",
            r#"import org.testng.annotations.Test;
import org.testng.annotations.DataProvider;
import static org.testng.Assert.*;

public class FunctionNameTest {
    @Test
    public void shouldRETURN_EXPECTED_whenSCENARIO() {
        var result = functionName(INPUT);
        assertEquals(result, EXPECTED);
    }

    @DataProvider(name = "testData")
    public Object[][] data() {
        return new Object[][] { {INPUT1, EXPECTED1}, {INPUT2, EXPECTED2} };
    }

    @Test(dataProvider = "testData")
    public void parametrized(InputType input, OutputType expected) {
        assertEquals(functionName(input), expected);
    }
}"#,
        )),
        "integration" => Some((
            "Use @BeforeClass/@AfterClass for setup. groups={\"integration\"} for tagging. dependsOnMethods for ordering.",
            r#"import org.testng.annotations.*;
import static org.testng.Assert.*;

@Test(groups = {"integration"})
public class FunctionNameIntegrationTest {
    private Connection conn;

    @BeforeClass
    public void setup() { conn = createConnection(); }

    @AfterClass
    public void teardown() { conn.close(); }

    @Test
    public void shouldSCENARIO() {
        var result = functionName(conn);
        assertNotNull(result);
    }
}"#,
        )),
        _ => None,
    }
}

fn cucumber_template(tt: &str) -> Option<(&'static str, &'static str)> {
    match tt {
        "functional" | "e2e" => Some((
            "Use Feature/Scenario/Given-When-Then in .feature files. Step definitions in language-specific files. Scenario Outline + Examples for data-driven.",
            r#"Feature: FEATURE_NAME

  Scenario: SCENARIO_DESCRIPTION
    Given PRECONDITION
    When ACTION is performed
    Then EXPECTED_OUTCOME should occur

  Scenario Outline: PARAMETRIZED_SCENARIO
    Given input is "<input>"
    When processed
    Then result should be "<expected>"

    Examples:
      | input  | expected |
      | CASE1  | RESULT1  |
      | CASE2  | RESULT2  |"#,
        )),
        _ => None,
    }
}
