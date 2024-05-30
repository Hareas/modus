<br />
<div align="center">
  <a href="https://github.com/Hareas/modus">
    <img src="https://raw.githubusercontent.com/Hareas/modus/6d5283ff95528fdbc1671d4c769024f0eb63c9f8/logo.svg" alt="Logo" width="100" height="100">
  </a>

  <h1 align="center">Modus</h1>

  <p align="center">
    An API and a library for long term portfolio performance and option valuation written in Rust and using the Actix framework.
    <br />
    <a href="https://hareas.github.io/modus/doc/modus/"><strong>Docs</strong></a>
    <br />
    <br />
  </p>
</div>

# About The Project

This project has two main purposes:

* To provide portfolio performance from historical data, irrespective of the amount invested, using data from Yahoo Finance.

* To calculate option value and provide optimal betting size.



# Build from source

To build from source you need to have the Rust toolchain installed.

```
cargo build --release
```

# Usage

Library documentation and usage is on the [docs](https://hareas.github.io/modus/doc/modus/).

The following endpoints are available:

* GET ```/equities/returns``` - Returns the historical performance in percentage since the beginning, daily.
* GET ```/options/bs``` - Calculates the theoretical value using the Black-Scholes formula.
* GET ```/options/kelly``` - Experimental. Gives the optimal betting size based on the Kelly Criterion when the price is different for the Black-Scholes value.
* GET ```/options/mc``` - Calculates the theoretical value doing a Monte Carlo simulation.

Sample JSON the body of the petition must have for /equities/returns, sell data is optional (meaning it hasn't been sold) and al price and quantity information must be split-adjusted:
```json
{
    "portfolio": [
        {
            "ticker": "ITX.MC",
            "buy": {
                "date": {
                    "year": 2020,
                    "month": 1,
                    "day": 1
                },
                "price": 31.7
            },
            "sell": {
                "date": {
                    "year": 2020,
                    "month": 12,
                    "day": 14
                },
                "price": 27.02
            },
            "quantity": 30
        },
        {
            "ticker": "MSFT",
            "buy": {
                "date": {
                    "year": 2020,
                    "month": 9,
                    "day": 21
                },
                "price": 198.3
            },
            "quantity": 15
        }
    ]
}
```

Sample JSON for the /options endpoints, market_price is only required for /kelly:

```json
{
    "form": "Call",
    "underlying": 15,
    "strike": 18,
    "maturity": 1,
    "volatility": 0.35,
    "rfr": 0.03,
    "market_price": 1.13
}
```

```form``` is the type of option, either ```Call``` or ```Put```, ```underlying``` is the price of the underlying, ```rfr``` is the risk-free rate, ```maturity``` is the time to maturity and ```market_price``` is the market price of the option. The measures are not relevant as long as they are consistent: From example if the risk-free rate is in years, the time to maturity must be as well.

# License
This project uses the MIT license. I don't care what you do with it and you don't need to give any credit.
